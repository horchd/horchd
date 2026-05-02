export interface DaemonStatus {
  running: boolean;
  audio_fps: number;
  score_fps: number;
  /** Smoothed peak |sample| of the most recent cpal callback, in [0, 1]. */
  mic_level: number;
}

export interface WakewordRow {
  name: string;
  threshold: number;
  model: string;
  enabled: boolean;
  cooldown_ms: number;
}

export interface DetectedPayload {
  name: string;
  score: number;
  timestamp_us: number;
  received_unix_ms: number;
}

export interface ScorePayload {
  name: string;
  score: number;
}

export interface FireRecord {
  name: string;
  score: number;
  ts_ms: number;
}

export type SampleKind = "positive" | "negative";

export interface TrainingSample {
  kind: SampleKind;
  path: string;
  ts_ms: number;
  size: number;
  duration_ms: number;
  sample_rate: number;
}

export interface TrainingWord {
  name: string;
  positive: number;
  negative: number;
  target_phrase: string | null;
}

export type TrainEvent =
  | { kind: "log"; line: string }
  | { kind: "status"; payload: Record<string, unknown> };

export interface TrainingEnvStatus {
  uv_version: string | null;
  python_env_dir: string;
  python_path: string | null;
  openwakeword_installed: boolean;
  package_dir: string | null;
  negatives_features_path: string;
  negatives_present: boolean;
}
