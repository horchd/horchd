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
  saveTrainingSample: (
    name: string,
    kind: SampleKind,
    mime: string,
    data: Uint8Array,
  ) =>
    invoke<TrainingSample>("save_training_sample", {
      name,
      kind,
      mime,
      data: Array.from(data),
    }),
  listTrainingSamples: (name: string) =>
    invoke<TrainingSample[]>("list_training_samples", { name }),
  listTrainingWords: () => invoke<TrainingWord[]>("list_training_words"),
  deleteTrainingSample: (path: string) =>
    invoke<void>("delete_training_sample", { path }),
  trainWakeword: (name: string) => invoke<string>("train_wakeword", { name }),
};

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
