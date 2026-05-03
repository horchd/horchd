//! Per-connection Wyoming state machine.
//!
//! Three modes ([`WyomingMode`]):
//!
//! - **`local-mic`** — the daemon owns the audio source. Detections from
//!   the live mic pipeline fan out to every connected client. Client
//!   `audio-*` events are tolerated but ignored.
//! - **`wyoming-server`** — each client streams its own audio via
//!   `audio-chunk`s. Per-connection isolated inference state, no
//!   relationship to the local mic. This is the standard Wyoming
//!   wake-word topology that HA's voice pipeline expects.
//! - **`hybrid`** — both at once. Live mic detections fan out *and*
//!   client-pushed audio gets its own per-connection pipeline.
//!
//! Per-connection inference loads a fresh `Preprocessor` (~10 MB) and
//! one `Classifier` per wakeword (~80 KB) at the first `audio-start`.
//! That's the same isolation pattern Phase B uses for `ProcessAudio`.

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use horchd_client::{AudioSource as _, Detection as RawDetection, DetectionSink, WyomingMode};
use horchd_wyoming::audio::{AudioChunk, AudioStart};
use horchd_wyoming::event::{Eventable, read_event, write_event};
use horchd_wyoming::wake::{Detect, Detection as WyoDetection};
use tokio::io::{AsyncBufRead, AsyncWrite};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::audio::WyomingSource;
use crate::detector::Detector;
use crate::inference::{Classifier, InferencePipeline, Preprocessor};
use crate::pipeline::TransientPipeline;
use crate::sink::MpscSink;
use crate::wyoming::info::build_info;
use crate::wyoming::listener::ServerCtx;

/// One Mode-2 / Hybrid per-connection inference pipeline.
struct ClientPipeline {
    pcm_tx: mpsc::Sender<Vec<i16>>,
    det_rx: mpsc::UnboundedReceiver<RawDetection>,
    /// Aborted on drop as a backup; clean shutdown happens via `pcm_tx`
    /// drop → frame channel close → `pipeline.run` returns.
    task: JoinHandle<()>,
    started_at: Instant,
}

impl Drop for ClientPipeline {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Drive a single Wyoming connection until either the peer disconnects
/// or one of the underlying broadcasts closes.
pub async fn handle<R, W>(
    mut reader: R,
    mut writer: W,
    ctx: Arc<ServerCtx>,
    peer: String,
) -> Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    tracing::info!(%peer, mode = ?ctx.mode, "Wyoming client connected");

    let subscribe_to_live = matches!(ctx.mode, WyomingMode::LocalMic | WyomingMode::Hybrid);
    let allow_client_audio = matches!(ctx.mode, WyomingMode::WyomingServer | WyomingMode::Hybrid);

    let mut live_detections = subscribe_to_live.then(|| ctx.pipeline.subscribe_detections());
    let mut filter: Option<Vec<String>> = None;
    let session_started = Instant::now();
    let mut client: Option<ClientPipeline> = None;

    loop {
        // The two optional select arms (`live_detections.recv()` and
        // `client.det_rx.recv()`) gate themselves on `if .is_some()` so
        // a None branch never gets polled.
        tokio::select! {
            biased;
            evt = read_event(&mut reader) => match evt? {
                None => break,
                Some(evt) => match evt.event_type.as_str() {
                    "describe" => {
                        let info = {
                            let s = ctx.state.lock().await;
                            build_info(&s)
                        };
                        write_event(&mut writer, &info.into_event()).await?;
                    }
                    "detect" => {
                        let det = Detect::from_event(&evt)?;
                        filter = (!det.names.is_empty()).then_some(det.names);
                        tracing::debug!(%peer, ?filter, "wakeword filter set");
                    }
                    "audio-start" if allow_client_audio => {
                        let start = AudioStart::from_event(&evt)?;
                        if let Err(err) = WyomingSource::validate_format(&start) {
                            tracing::warn!(%peer, ?err, "rejecting client audio");
                            // No standard Wyoming "error" event — log
                            // and keep the session open so the client
                            // can re-issue a corrected audio-start.
                            continue;
                        }
                        if client.is_none() {
                            match spawn_client_pipeline(&ctx, &peer).await {
                                Ok(p) => {
                                    tracing::info!(%peer, "per-connection pipeline started");
                                    client = Some(p);
                                }
                                Err(err) => {
                                    tracing::error!(%peer, ?err, "failed to start client pipeline");
                                }
                            }
                        }
                    }
                    "audio-chunk" if allow_client_audio => {
                        if let Some(c) = &client {
                            let chunk = AudioChunk::from_event(&evt)?;
                            let samples = crate::audio::wyoming::decode_pcm_i16_le(&chunk.audio);
                            // Bounded send: blocks the read loop briefly
                            // if the inference loop has fallen behind. No
                            // silent drops — Wyoming clients deserve to
                            // feel real backpressure.
                            if c.pcm_tx.send(samples).await.is_err() {
                                tracing::warn!(%peer, "client pipeline closed; dropping chunk");
                                client = None;
                            }
                        } else {
                            tracing::debug!(%peer, "audio-chunk before audio-start; ignoring");
                        }
                    }
                    "audio-stop" if allow_client_audio => {
                        if let Some(c) = client.take() {
                            tracing::info!(%peer, "client pipeline ended");
                            // Drop closes pcm_tx → framer → frame channel → pipeline.run.
                            drop(c);
                        }
                    }
                    "audio-start" | "audio-chunk" | "audio-stop" => {
                        // local-mic mode: client audio is irrelevant.
                        tracing::debug!(%peer, ty = %evt.event_type, "ignored in local-mic mode");
                    }
                    other => {
                        tracing::debug!(%peer, ty = %other, "unhandled event type");
                    }
                },
            },
            recv = async { live_detections.as_mut().expect("checked").recv().await }, if live_detections.is_some() => {
                match recv {
                    Ok(det) => {
                        if filter_match(filter.as_ref(), &det.name) {
                            let ts_ms = u64::try_from(session_started.elapsed().as_millis()).unwrap_or(u64::MAX);
                            write_detection(&mut writer, &det, ts_ms, &peer).await?;
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!(%peer, skipped = n, "Wyoming live broadcast lagged");
                    }
                    Err(RecvError::Closed) => {
                        live_detections = None;
                    }
                }
            }
            recv = async { client.as_mut().expect("checked").det_rx.recv().await }, if client.is_some() => {
                match recv {
                    Some(det) => {
                        if filter_match(filter.as_ref(), &det.name) {
                            let ts_ms = client
                                .as_ref()
                                .map(|c| u64::try_from(c.started_at.elapsed().as_millis()).unwrap_or(u64::MAX))
                                .unwrap_or_default();
                            write_detection(&mut writer, &det, ts_ms, &peer).await?;
                        }
                    }
                    None => {
                        // client pipeline ended (e.g. inference task panicked).
                        tracing::info!(%peer, "client detection channel closed");
                        client = None;
                    }
                }
            }
        }
    }

    tracing::info!(%peer, "Wyoming client disconnected");
    Ok(())
}

fn filter_match(filter: Option<&Vec<String>>, name: &str) -> bool {
    match filter {
        Some(names) => names.iter().any(|n| n == name),
        None => true,
    }
}

async fn write_detection<W>(
    writer: &mut W,
    det: &RawDetection,
    timestamp_ms: u64,
    peer: &str,
) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let wyo = WyoDetection {
        name: det.name.clone(),
        timestamp: Some(timestamp_ms),
        speaker: None,
    };
    write_event(writer, &wyo.into_event()).await?;
    tracing::debug!(%peer, name = %det.name, score = det.score, "forwarded detection");
    Ok(())
}

/// Build a fresh per-connection inference pipeline: snapshot the
/// current wakeword config, load isolated ONNX sessions off-runtime,
/// and spawn a [`TransientPipeline`] driven by a [`WyomingSource`].
///
/// Setup cost: ~200 ms wall-clock for the model load. Memory: one extra
/// Preprocessor (~10 MB) + N Classifiers (~80 KB each).
async fn spawn_client_pipeline(ctx: &ServerCtx, peer: &str) -> Result<ClientPipeline> {
    let (shared_models, wakewords) = {
        let s = ctx.state.lock().await;
        (
            s.config.engine.shared_models.clone(),
            s.config.wakewords.clone(),
        )
    };

    let inference = {
        let shared_models = shared_models.clone();
        let wakewords = wakewords.clone();
        tokio::task::spawn_blocking(move || -> Result<InferencePipeline> {
            let preprocessor =
                Preprocessor::new(&shared_models.melspectrogram, &shared_models.embedding)?;
            let classifiers = wakewords
                .iter()
                .map(|w| Classifier::load(w.name.clone(), &w.model))
                .collect::<Result<Vec<_>>>()?;
            Ok(InferencePipeline::new(preprocessor, classifiers))
        })
        .await
        .context("inference setup join")?
        .context("loading isolated inference state")?
    };

    let detectors: Vec<Detector> = wakewords
        .iter()
        .map(|w| Detector::new(w.name.clone(), w.threshold, w.cooldown_ms, w.enabled))
        .collect();

    let (pcm_tx, mut source) = WyomingSource::new(peer);
    let frames = source.start().context("starting WyomingSource")?;

    let (mpsc_sink, det_rx) = MpscSink::new();
    let sinks: Vec<Arc<dyn DetectionSink>> = vec![Arc::new(mpsc_sink)];

    let task = tokio::spawn(async move {
        // Keep `source` alive for the pipeline's lifetime — it owns the
        // framer JoinHandle whose Drop would abort the framer task.
        let _source = source;
        TransientPipeline::new(inference, detectors, sinks)
            .run(frames)
            .await;
    });

    Ok(ClientPipeline {
        pcm_tx,
        det_rx,
        task,
        started_at: Instant::now(),
    })
}
