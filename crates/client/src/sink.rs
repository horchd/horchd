//! Detection sink abstraction.

use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub name: String,
    pub score: f64,
    /// `CLOCK_MONOTONIC` microseconds since boot — matches the
    /// `xyz.horchd.Daemon1.Detected` D-Bus signal payload.
    pub timestamp_us: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreSnapshot {
    pub name: String,
    pub score: f64,
}

/// Destination for wakeword detections and (optionally) live score samples.
///
/// Sinks are typically wrapped in `Arc` so several pipelines can share
/// one transport (e.g. one D-Bus connection feeding many sources).
#[async_trait]
pub trait DetectionSink: Send + Sync {
    async fn emit_detection(&self, det: &Detection);

    /// Default ignores the snapshot — override for live-meter feeds.
    /// Slow sinks must buffer or drop, never back-pressure the pipeline.
    async fn emit_snapshot(&self, _snap: &ScoreSnapshot) {}

    fn name(&self) -> &'static str;
}
