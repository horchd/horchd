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
