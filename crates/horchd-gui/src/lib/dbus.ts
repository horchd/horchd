// Typed wrappers around the Tauri commands defined in
// `crates/horchd-gui/src/commands.rs`. Falls back to deterministic mock
// data when the page is loaded outside of Tauri (handy for `bun run dev`
// in a regular browser while iterating on design).

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";

import type { DaemonStatus, DetectedPayload, ScorePayload, WakewordRow } from "./types";

const inTauri =
  typeof window !== "undefined" &&
  // @ts-expect-error injected at runtime by Tauri
  typeof window.__TAURI_INTERNALS__ !== "undefined";

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) return tauriInvoke<T>(cmd, args);
  return mockInvoke<T>(cmd, args);
}

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
};

export async function onDetected(
  cb: (payload: DetectedPayload) => void,
): Promise<UnlistenFn> {
  if (inTauri) {
    return tauriListen<DetectedPayload>("horchd://detected", (e) => cb(e.payload));
  }
  // Out-of-Tauri: simulate occasional fires for design iteration.
  const id = window.setInterval(() => {
    const names = mockState.wakes.map((w) => w.name).filter(Boolean);
    if (!names.length) return;
    const name = names[Math.floor(Math.random() * names.length)];
    cb({
      name,
      score: 0.55 + Math.random() * 0.4,
      timestamp_us: Date.now() * 1000,
      received_unix_ms: Date.now(),
    });
  }, 7000);
  return async () => window.clearInterval(id);
}

export async function onScore(
  cb: (payload: ScorePayload) => void,
): Promise<UnlistenFn> {
  if (inTauri) {
    return tauriListen<ScorePayload>("horchd://score", (e) => cb(e.payload));
  }
  // Out-of-Tauri: drift each wake's score with smooth pink-noise so the
  // meter exercises both below- and above-threshold states.
  const drift: Record<string, number> = {};
  const id = window.setInterval(() => {
    for (const w of mockState.wakes) {
      const prev = drift[w.name] ?? Math.random() * 0.5;
      const next = Math.max(0, Math.min(1, prev + (Math.random() - 0.5) * 0.15));
      drift[w.name] = next;
      cb({ name: w.name, score: next });
    }
  }, 200);
  return async () => window.clearInterval(id);
}

// ----- mock backend (browser-only dev preview) -----

const mockState = {
  audioFps: 12.5,
  scoreFps: 12.46,
  wakes: [
    { name: "lyna", threshold: 0.5, model: "/home/you/.local/share/horchd/models/lyna.onnx", enabled: true, cooldown_ms: 1500 },
    { name: "hey_jarvis", threshold: 0.65, model: "/home/you/.local/share/horchd/models/hey_jarvis_v0.1.onnx", enabled: true, cooldown_ms: 1500 },
    { name: "wetter", threshold: 0.55, model: "/home/you/.local/share/horchd/models/wetter.onnx", enabled: false, cooldown_ms: 2000 },
  ] as WakewordRow[],
};

async function mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  // Tiny jitter so the gauges look alive in the browser preview.
  const jitter = (b: number) => b + (Math.random() - 0.5) * 0.06;
  switch (cmd) {
    case "get_status":
      return {
        running: true,
        audio_fps: jitter(mockState.audioFps),
        score_fps: jitter(mockState.scoreFps),
        // simulate breathing input level for the meter preview
        mic_level: 0.04 + Math.abs(Math.sin(Date.now() / 700)) * 0.2,
      } as T;
    case "list_wakewords":
      return mockState.wakes.map((w) => ({ ...w })) as unknown as T;
    case "set_threshold": {
      const w = mockState.wakes.find((x) => x.name === args?.name);
      if (w) w.threshold = args?.value as number;
      return undefined as T;
    }
    case "set_enabled": {
      const w = mockState.wakes.find((x) => x.name === args?.name);
      if (w) w.enabled = args?.enabled as boolean;
      return undefined as T;
    }
    case "set_cooldown": {
      const w = mockState.wakes.find((x) => x.name === args?.name);
      if (w) w.cooldown_ms = args?.ms as number;
      return undefined as T;
    }
    case "add_wakeword": {
      const name = args?.name as string;
      if (mockState.wakes.some((w) => w.name === name)) {
        throw new Error(`wakeword "${name}" already exists`);
      }
      mockState.wakes.push({
        name,
        model: args?.model as string,
        threshold: args?.threshold as number,
        cooldown_ms: args?.cooldownMs as number,
        enabled: true,
      });
      return undefined as T;
    }
    case "remove_wakeword":
      mockState.wakes = mockState.wakes.filter((w) => w.name !== args?.name);
      return undefined as T;
    case "reload":
      return undefined as T;
    case "models_dir":
      return "/home/you/.local/share/horchd/models" as T;
    case "import_wakeword": {
      const name = args?.name as string;
      if (mockState.wakes.some((w) => w.name === name)) {
        throw new Error(`wakeword "${name}" already exists`);
      }
      const dest = `/home/you/.local/share/horchd/models/${name}.onnx`;
      mockState.wakes.push({
        name,
        model: dest,
        threshold: args?.threshold as number,
        cooldown_ms: args?.cooldownMs as number,
        enabled: true,
      });
      return dest as T;
    }
    default:
      console.warn("[mock] unknown command", cmd, args);
      return undefined as T;
  }
}
