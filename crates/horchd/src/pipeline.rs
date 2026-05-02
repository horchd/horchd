//! Wakeword detection pipeline: drives an [`AudioSource`]-derived frame
//! receiver through ONNX inference and fans the resulting Detection /
//! ScoreSnapshot events out to subscribed sinks via internal broadcast
//! channels.
//!
//! Subscribe sinks with [`Pipeline::add_sink`] **before** calling
//! [`Pipeline::run`] — events fired before a sink subscribes are lost.

use std::sync::Arc;
use std::time::{Duration, Instant};

use horchd_client::{AudioFrame, Detection, DetectionSink, ScoreSnapshot};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use crate::inference::InferenceStats;
use crate::state::SharedState;

/// Inference fires ~12.5 Hz; throttling snapshots to 5 Hz keeps the bus
/// quiet while still feeling live in a UI meter.
const SCORE_SNAPSHOT_INTERVAL: Duration = Duration::from_millis(200);

const DETECTION_BROADCAST_CAPACITY: usize = 64;
const SNAPSHOT_BROADCAST_CAPACITY: usize = 256;

pub struct Pipeline {
    state: SharedState,
    stats: Arc<InferenceStats>,
    detections: broadcast::Sender<Detection>,
    snapshots: broadcast::Sender<ScoreSnapshot>,
}

impl Pipeline {
    pub fn new(state: SharedState, stats: Arc<InferenceStats>) -> Self {
        let (detections, _) = broadcast::channel(DETECTION_BROADCAST_CAPACITY);
        let (snapshots, _) = broadcast::channel(SNAPSHOT_BROADCAST_CAPACITY);
        Self {
            state,
            stats,
            detections,
            snapshots,
        }
    }

    /// Spawn an emitter task that forwards every Detection and
    /// ScoreSnapshot to `sink`. Returned `JoinHandle` can be aborted to
    /// stop the sink without affecting other subscribers.
    ///
    /// Cancellation: `JoinHandle::abort` is **hard** — the task is
    /// dropped at the next `.await` without giving the sink a chance to
    /// flush. For sinks that need a graceful drain (Wyoming TCP
    /// connections in Phase D), wire a `tokio_util::sync::CancellationToken`
    /// through the sink and select on it in the emitter loop.
    pub fn add_sink(&self, sink: Arc<dyn DetectionSink>) -> JoinHandle<()> {
        spawn_sink_emitter(
            sink,
            self.detections.subscribe(),
            self.snapshots.subscribe(),
        )
    }

    /// Drive `frames` through inference. Returns when the receiver
    /// closes — typically because the source was dropped or its
    /// underlying transport ended.
    pub async fn run(&self, mut frames: mpsc::Receiver<AudioFrame>) {
        let mut last_snapshot: Option<Instant> = None;
        while let Some(frame) = frames.recv().await {
            let mut s = self.state.lock().await;
            let started = Instant::now();
            let result = tokio::task::block_in_place(|| s.pipeline.process(&frame));
            let elapsed = started.elapsed();
            let scores = match result {
                Ok(scores) => scores,
                Err(err) => {
                    tracing::error!(?err, "inference failed");
                    continue;
                }
            };
            self.stats.record_score(elapsed);

            let now = Instant::now();
            let snapshot_due = last_snapshot
                .map(|t| now.duration_since(t) >= SCORE_SNAPSHOT_INTERVAL)
                .unwrap_or(true);

            for (det, (name, score)) in s.detectors.iter_mut().zip(scores.iter()) {
                debug_assert_eq!(name, &det.name, "detector/classifier order mismatch");
                let score_f64 = f64::from(*score);
                if snapshot_due {
                    let _ = self.snapshots.send(ScoreSnapshot {
                        name: name.clone(),
                        score: score_f64,
                    });
                }
                let Some(event) = det.update(score_f64, now) else {
                    continue;
                };
                tracing::info!(
                    name = %event.name,
                    score = event.score,
                    ts_us = event.timestamp_us,
                    "wakeword detected"
                );
                let _ = self.detections.send(event);
            }
            if snapshot_due {
                last_snapshot = Some(now);
            }
        }
        tracing::warn!("audio frame channel closed");
    }
}

/// One-shot inference pipeline that owns its own [`InferencePipeline`]
/// and detector set — no `SharedState`, no broadcast fan-out.
///
/// Use this when running a finite audio source (file, replay) without
/// disturbing the live mic pipeline. Sinks are called sequentially per
/// frame; for [`ProcessAudio`] we hand it a single
/// [`crate::sink::MpscSink`] which is unbounded and never blocks.
///
/// Cooldown caveat: [`Detector`] uses wall-clock `Instant` for its
/// cooldown gate. When a file processes faster than realtime (typical
/// — 1 s of audio takes ~80 ms of inference), two detections that are
/// 1.5 s apart in the file may collapse to ~120 ms apart in wall time,
/// inside the cooldown window, and the second one is dropped. Acceptable
/// for v0.2.0; v0.3.0 should refactor `Detector::update` to take a
/// generic time source.
pub struct TransientPipeline {
    inference: crate::inference::InferencePipeline,
    detectors: Vec<crate::detector::Detector>,
    sinks: Vec<Arc<dyn DetectionSink>>,
}

impl TransientPipeline {
    pub fn new(
        inference: crate::inference::InferencePipeline,
        detectors: Vec<crate::detector::Detector>,
        sinks: Vec<Arc<dyn DetectionSink>>,
    ) -> Self {
        Self {
            inference,
            detectors,
            sinks,
        }
    }

    /// Run until `frames` closes. Returns once the last frame is processed.
    pub async fn run(mut self, mut frames: mpsc::Receiver<horchd_client::AudioFrame>) {
        while let Some(frame) = frames.recv().await {
            let result = tokio::task::block_in_place(|| self.inference.process(&frame));
            let scores = match result {
                Ok(s) => s,
                Err(err) => {
                    tracing::error!(?err, "transient inference failed");
                    continue;
                }
            };
            let now = std::time::Instant::now();
            for (det, (name, score)) in self.detectors.iter_mut().zip(scores.iter()) {
                debug_assert_eq!(name, &det.name, "detector/classifier order mismatch");
                let Some(event) = det.update(f64::from(*score), now) else {
                    continue;
                };
                for sink in &self.sinks {
                    sink.emit_detection(&event).await;
                }
            }
        }
    }
}

/// Spawn the per-sink subscriber loop. Pulled out of `Pipeline::add_sink`
/// so tests can drive it with their own broadcasts.
pub(crate) fn spawn_sink_emitter(
    sink: Arc<dyn DetectionSink>,
    mut detections: broadcast::Receiver<Detection>,
    mut snapshots: broadcast::Receiver<ScoreSnapshot>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        use broadcast::error::RecvError;
        loop {
            tokio::select! {
                res = detections.recv() => match res {
                    Ok(det) => sink.emit_detection(&det).await,
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!(sink = sink.name(), skipped = n, "detection sink lagged")
                    }
                    Err(RecvError::Closed) => break,
                },
                res = snapshots.recv() => match res {
                    Ok(snap) => sink.emit_snapshot(&snap).await,
                    Err(RecvError::Lagged(n)) => {
                        tracing::debug!(sink = sink.name(), skipped = n, "snapshot sink lagged")
                    }
                    Err(RecvError::Closed) => break,
                },
            }
        }
        tracing::info!(sink = sink.name(), "sink emitter exited");
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use async_trait::async_trait;

    use super::*;

    #[derive(Default)]
    struct RecordingSink {
        detections: Mutex<Vec<Detection>>,
        snapshots: Mutex<Vec<ScoreSnapshot>>,
    }

    #[async_trait]
    impl DetectionSink for RecordingSink {
        async fn emit_detection(&self, det: &Detection) {
            self.detections.lock().unwrap().push(det.clone());
        }

        async fn emit_snapshot(&self, snap: &ScoreSnapshot) {
            self.snapshots.lock().unwrap().push(snap.clone());
        }

        fn name(&self) -> &'static str {
            "recording"
        }
    }

    async fn wait_until<F: Fn() -> bool>(predicate: F) {
        for _ in 0..100 {
            if predicate() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("predicate did not become true within 1 s");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fanout_delivers_detection_to_every_sink() {
        let (det_tx, _) = broadcast::channel::<Detection>(8);
        let (snap_tx, _) = broadcast::channel::<ScoreSnapshot>(8);

        let sink_a: Arc<RecordingSink> = Arc::default();
        let sink_b: Arc<RecordingSink> = Arc::default();
        spawn_sink_emitter(
            Arc::clone(&sink_a) as Arc<dyn DetectionSink>,
            det_tx.subscribe(),
            snap_tx.subscribe(),
        );
        spawn_sink_emitter(
            Arc::clone(&sink_b) as Arc<dyn DetectionSink>,
            det_tx.subscribe(),
            snap_tx.subscribe(),
        );

        let det = Detection {
            name: "alexa".to_string(),
            score: 0.9,
            timestamp_us: 1234,
        };
        det_tx.send(det.clone()).unwrap();

        wait_until(|| {
            sink_a.detections.lock().unwrap().len() == 1
                && sink_b.detections.lock().unwrap().len() == 1
        })
        .await;
        let expected = std::slice::from_ref(&det);
        assert_eq!(sink_a.detections.lock().unwrap().as_slice(), expected);
        assert_eq!(sink_b.detections.lock().unwrap().as_slice(), expected);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn snapshots_route_independently_from_detections() {
        let (det_tx, _) = broadcast::channel::<Detection>(8);
        let (snap_tx, _) = broadcast::channel::<ScoreSnapshot>(8);

        let sink: Arc<RecordingSink> = Arc::default();
        spawn_sink_emitter(
            Arc::clone(&sink) as Arc<dyn DetectionSink>,
            det_tx.subscribe(),
            snap_tx.subscribe(),
        );

        let snap = ScoreSnapshot {
            name: "alexa".into(),
            score: 0.4,
        };
        snap_tx.send(snap.clone()).unwrap();

        wait_until(|| !sink.snapshots.lock().unwrap().is_empty()).await;
        assert_eq!(sink.snapshots.lock().unwrap().as_slice(), &[snap]);
        assert!(sink.detections.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn emitter_exits_when_broadcasts_close() {
        let (det_tx, _) = broadcast::channel::<Detection>(8);
        let (snap_tx, _) = broadcast::channel::<ScoreSnapshot>(8);

        let sink: Arc<RecordingSink> = Arc::default();
        let handle = spawn_sink_emitter(
            Arc::clone(&sink) as Arc<dyn DetectionSink>,
            det_tx.subscribe(),
            snap_tx.subscribe(),
        );

        drop(det_tx);
        drop(snap_tx);

        tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("emitter task did not exit after channels closed")
            .expect("emitter task panicked");
    }
}
