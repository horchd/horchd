// Typed wrappers around the Tauri commands defined in
// `crates/horchd-gui/src-tauri/src/commands.rs`.
//
// When `bun run dev` is started in a plain browser (no Tauri shell) the
// Tauri `invoke` / `listen` APIs are absent. We swap in a deterministic
// mock backend so the SvelteKit page can be developed and design-iterated
// without a running daemon — see `mockBackend` below. README + the gui
// crate's README both advertise this.

import type { UnlistenFn } from "@tauri-apps/api/event";

import type {
  DaemonStatus,
  DetectedPayload,
  SampleKind,
  ScorePayload,
  TrainEvent,
  TrainingEnvStatus,
  TrainingSample,
  TrainingWord,
  WakewordRow,
} from "./types";

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

async function tauriListen<T>(event: string, cb: (e: { payload: T }) => void): Promise<UnlistenFn> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, cb);
}

function rpc<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) return mockBackend.invoke<T>(cmd, args);
  return tauriInvoke<T>(cmd, args);
}

async function listenEvent<T>(
  event: string,
  cb: (payload: T) => void,
): Promise<UnlistenFn> {
  if (!inTauri) return mockBackend.listen<T>(event, cb);
  return tauriListen<T>(event, (e) => cb(e.payload));
}

export const dbus = {
  status: () => rpc<DaemonStatus>("get_status"),
  list: () => rpc<WakewordRow[]>("list_wakewords"),
  setThreshold: (name: string, value: number, save: boolean) =>
    rpc<void>("set_threshold", { name, value, save }),
  setEnabled: (name: string, enabled: boolean, save: boolean) =>
    rpc<void>("set_enabled", { name, enabled, save }),
  setCooldown: (name: string, ms: number, save: boolean) =>
    rpc<void>("set_cooldown", { name, ms, save }),
  add: (name: string, model: string, threshold: number, cooldownMs: number) =>
    rpc<void>("add_wakeword", { name, model, threshold, cooldownMs }),
  /** Copies `sourcePath` into the canonical models dir, then registers. */
  import: (name: string, sourcePath: string, threshold: number, cooldownMs: number) =>
    rpc<string>("import_wakeword", { name, sourcePath, threshold, cooldownMs }),
  remove: (name: string) => rpc<void>("remove_wakeword", { name }),
  reload: () => rpc<void>("reload"),
  modelsDir: () => rpc<string>("models_dir"),
  listInputDevices: () => rpc<string[]>("list_input_devices"),
  setInputDevice: (name: string, save: boolean) =>
    rpc<void>("set_input_device", { name, save }),
  trainingDir: () => rpc<string>("training_dir"),
  /** Persist int16-PCM mono samples as a real .wav under the training dir. */
  saveTrainingSample: (
    name: string,
    kind: SampleKind,
    sampleRate: number,
    samples: Int16Array,
  ) =>
    rpc<TrainingSample>("save_training_sample", {
      name,
      kind,
      sampleRate,
      samples: Array.from(samples),
    }),
  saveWordMeta: (name: string, targetPhrase: string) =>
    rpc<void>("save_word_meta", { name, targetPhrase }),
  listTrainingSamples: (name: string) =>
    rpc<TrainingSample[]>("list_training_samples", { name }),
  listTrainingWords: () => rpc<TrainingWord[]>("list_training_words"),
  deleteTrainingSample: (path: string) =>
    rpc<void>("delete_training_sample", { path }),
  /** Returns raw WAV bytes; wrap in `new Blob([bytes], { type: 'audio/wav' })`. */
  readTrainingSample: (path: string) =>
    rpc<number[]>("read_training_sample", { path }).then(
      (arr) => new Uint8Array(arr),
    ),
  trainWakeword: (
    name: string,
    targetPhrase: string,
    opts?: { augmentPerRecording?: number; steps?: number },
  ) =>
    rpc<string>("train_wakeword", {
      name,
      targetPhrase,
      augmentPerRecording: opts?.augmentPerRecording,
      steps: opts?.steps,
    }),
  trainingEnvStatus: () => rpc<TrainingEnvStatus>("training_env_status"),
  setupTrainingEnv: () => rpc<string>("setup_training_env"),
  fetchNegatives: () => rpc<string>("fetch_negatives"),
  cancelSetup: () => rpc<void>("cancel_setup"),
  cancelTraining: () => rpc<void>("cancel_training"),
};

export async function onTrain(cb: (payload: TrainEvent) => void): Promise<UnlistenFn> {
  return listenEvent<TrainEvent>("horchd://train", cb);
}

export async function onSetup(cb: (payload: TrainEvent) => void): Promise<UnlistenFn> {
  return listenEvent<TrainEvent>("horchd://setup", cb);
}

export async function onDetected(
  cb: (payload: DetectedPayload) => void,
): Promise<UnlistenFn> {
  return listenEvent<DetectedPayload>("horchd://detected", cb);
}

export async function onScore(cb: (payload: ScorePayload) => void): Promise<UnlistenFn> {
  return listenEvent<ScorePayload>("horchd://score", cb);
}

// ---------------------------------------------------------------------------
// Browser standalone mock backend.
//
// Deterministic enough that visual states can be designed against it.
// Not a substitute for actually running the daemon — calls that mutate
// (set_*, add_wakeword, train_wakeword, …) just return `undefined` after
// a small delay.
// ---------------------------------------------------------------------------

type Listener = (payload: unknown) => void;

const mockBackend = (() => {
  const wakes: WakewordRow[] = [
    { name: "alexa", threshold: 0.5, model: "/mock/alexa.onnx", enabled: true, cooldown_ms: 1500 },
    { name: "hey_jarvis", threshold: 0.55, model: "/mock/hey_jarvis.onnx", enabled: true, cooldown_ms: 1200 },
  ];
  const listeners = new Map<string, Set<Listener>>();
  const addListener = (e: string, cb: Listener) => {
    if (!listeners.has(e)) listeners.set(e, new Set());
    listeners.get(e)!.add(cb);
    return () => {
      listeners.get(e)?.delete(cb);
    };
  };
  // Drive a slow, repeatable Score stream so meters look alive.
  if (typeof window !== "undefined") {
    let t = 0;
    setInterval(() => {
      t += 0.2;
      for (const w of wakes) {
        const score = 0.05 + 0.5 * (Math.sin(t + w.name.length) + 1) * 0.5;
        const subs = listeners.get("horchd://score");
        subs?.forEach((cb) => cb({ name: w.name, score }));
      }
    }, 200);
  }
  return {
    invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
      const sleep = new Promise<void>((r) => setTimeout(r, 30));
      return sleep.then((): T => {
        switch (cmd) {
          case "get_status":
            return {
              running: true,
              audio_fps: 12.5,
              score_fps: 12.5,
              mic_level: 0.05 + Math.random() * 0.05,
            } as T;
          case "list_wakewords":
            return wakes.slice() as unknown as T;
          case "models_dir":
            return "/mock/models" as unknown as T;
          case "training_dir":
            return "/mock/training" as unknown as T;
          case "training_env_status":
            return {
              uv_version: null,
              python_env_dir: "/mock/python-env",
              python_path: null,
              openwakeword_installed: false,
              package_dir: "/mock/python",
              negatives_features_path: "/mock/negatives.npy",
              negatives_present: false,
            } as unknown as T;
          case "list_input_devices":
            return ["default", "Built-in Microphone"] as unknown as T;
          case "list_training_words":
            return [] as unknown as T;
          case "list_training_samples":
            return [] as unknown as T;
          default:
            // remove_wakeword / set_* / add / cancel_* / etc. — no-op
            void args;
            return undefined as unknown as T;
        }
      });
    },
    listen<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
      return Promise.resolve(addListener(event, cb as unknown as Listener));
    },
  };
})();

export const isStandalone = !inTauri;
