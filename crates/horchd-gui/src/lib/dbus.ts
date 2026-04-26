// Typed wrappers around the Tauri commands defined in
// `crates/horchd-gui/src-tauri/src/commands.rs`. Tauri-only — no
// browser-side mock backend; the UI is meant to run inside the Tauri
// shell against a live daemon.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

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

export const dbus = {
  status: () => invoke<DaemonStatus>("get_status"),
  list: () => invoke<WakewordRow[]>("list_wakewords"),
  setThreshold: (name: string, value: number, save: boolean) =>
    invoke<void>("set_threshold", { name, value, save }),
  setEnabled: (name: string, enabled: boolean, save: boolean) =>
    invoke<void>("set_enabled", { name, enabled, save }),
  setCooldown: (name: string, ms: number, save: boolean) =>
    invoke<void>("set_cooldown", { name, ms, save }),
  add: (name: string, model: string, threshold: number, cooldownMs: number) =>
    invoke<void>("add_wakeword", { name, model, threshold, cooldownMs }),
  /** Copies `sourcePath` into the canonical models dir, then registers. */
  import: (name: string, sourcePath: string, threshold: number, cooldownMs: number) =>
    invoke<string>("import_wakeword", { name, sourcePath, threshold, cooldownMs }),
  remove: (name: string) => invoke<void>("remove_wakeword", { name }),
  reload: () => invoke<void>("reload"),
  modelsDir: () => invoke<string>("models_dir"),
  listInputDevices: () => invoke<string[]>("list_input_devices"),
  setInputDevice: (name: string, save: boolean) =>
    invoke<void>("set_input_device", { name, save }),
  trainingDir: () => invoke<string>("training_dir"),
  /** Persist int16-PCM mono samples as a real .wav under the training dir. */
  saveTrainingSample: (
    name: string,
    kind: SampleKind,
    sampleRate: number,
    samples: Int16Array,
  ) =>
    invoke<TrainingSample>("save_training_sample", {
      name,
      kind,
      sampleRate,
      samples: Array.from(samples),
    }),
  saveWordMeta: (name: string, targetPhrase: string) =>
    invoke<void>("save_word_meta", { name, targetPhrase }),
  listTrainingSamples: (name: string) =>
    invoke<TrainingSample[]>("list_training_samples", { name }),
  listTrainingWords: () => invoke<TrainingWord[]>("list_training_words"),
  deleteTrainingSample: (path: string) =>
    invoke<void>("delete_training_sample", { path }),
  /** Returns raw WAV bytes; wrap in `new Blob([bytes], { type: 'audio/wav' })`. */
  readTrainingSample: (path: string) =>
    invoke<number[]>("read_training_sample", { path }).then(
      (arr) => new Uint8Array(arr),
    ),
  trainWakeword: (
    name: string,
    targetPhrase: string,
    opts?: { augmentPerRecording?: number; steps?: number },
  ) =>
    invoke<string>("train_wakeword", {
      name,
      targetPhrase,
      augmentPerRecording: opts?.augmentPerRecording,
      steps: opts?.steps,
    }),
  trainingEnvStatus: () => invoke<TrainingEnvStatus>("training_env_status"),
  setupTrainingEnv: () => invoke<string>("setup_training_env"),
  fetchNegatives: () => invoke<string>("fetch_negatives"),
};

export async function onTrain(
  cb: (payload: TrainEvent) => void,
): Promise<UnlistenFn> {
  return listen<TrainEvent>("horchd://train", (e) => cb(e.payload));
}

export async function onSetup(
  cb: (payload: TrainEvent) => void,
): Promise<UnlistenFn> {
  return listen<TrainEvent>("horchd://setup", (e) => cb(e.payload));
}

export async function onDetected(
  cb: (payload: DetectedPayload) => void,
): Promise<UnlistenFn> {
  return listen<DetectedPayload>("horchd://detected", (e) => cb(e.payload));
}

export async function onScore(
  cb: (payload: ScorePayload) => void,
): Promise<UnlistenFn> {
  return listen<ScorePayload>("horchd://score", (e) => cb(e.payload));
}
