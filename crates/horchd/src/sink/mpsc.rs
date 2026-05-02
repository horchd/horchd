//! In-memory sink: pushes detections into an mpsc channel for batch
//! retrieval. Used by `ProcessAudio` to collect all hits from a finite
//! audio source.

use async_trait::async_trait;
use horchd_client::{Detection, DetectionSink, ScoreSnapshot};
use tokio::sync::mpsc;

pub struct MpscSink {
    detections: mpsc::UnboundedSender<Detection>,
}

impl MpscSink {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<Detection>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { detections: tx }, rx)
    }
}

#[async_trait]
impl DetectionSink for MpscSink {
    async fn emit_detection(&self, det: &Detection) {
        // Unbounded — never blocks the inference loop. Receiver-dropped
        // is silent: it means the caller already moved on.
        let _ = self.detections.send(det.clone());
    }

    async fn emit_snapshot(&self, _snap: &ScoreSnapshot) {
        // ProcessAudio doesn't care about per-frame snapshots.
    }

    fn name(&self) -> &'static str {
        "mpsc"
    }
}
