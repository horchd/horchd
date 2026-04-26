use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use horchd_core::{Config, WakewordEvent};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing_subscriber::EnvFilter;
use zbus::Connection;
use zbus::object_server::SignalEmitter;

mod audio;
mod detector;
mod inference;
mod persist;
mod service;
mod state;

const DBUS_NAME: &str = "xyz.horchd.Daemon";
const DBUS_PATH: &str = "/xyz/horchd/Daemon";

/// Tokio mpsc capacity for raw audio frames.
/// 16 frames * 80 ms = 1.28 s of headroom against inference back-pressure.
const AUDIO_CHANNEL_CAPACITY: usize = 16;

/// Broadcast capacity for `Detected` events.
const EVENT_BROADCAST_CAPACITY: usize = 64;

/// Broadcast capacity for the higher-rate `ScoreSnapshot` channel.
/// At ~5 Hz × N wakewords, 256 leaves dozens of seconds of headroom for
/// any slow subscriber.
const SCORE_BROADCAST_CAPACITY: usize = 256;

/// Minimum wall-clock gap between consecutive `ScoreSnapshot` emissions
/// per wakeword. Inference fires ~12.5 Hz; throttling to 5 Hz keeps the
/// bus quiet while still feeling live to a UI meter.
const SCORE_SNAPSHOT_INTERVAL: Duration = Duration::from_millis(200);

/// Stats log cadence.
const STATS_LOG_INTERVAL: Duration = Duration::from_secs(30);

/// Commands the D-Bus service handler can send back to `main` so audio
/// device hot-swaps run on the thread that owns the (`!Send`) cpal
/// `Stream`.
pub enum AudioCmd {
    List {
        reply: oneshot::Sender<Result<Vec<String>>>,
    },
    SetDevice {
        name: String,
        persist: bool,
        reply: oneshot::Sender<Result<()>>,
    },
}

#[derive(Debug, Parser)]
#[command(
    name = "horchd",
    version,
    about = "Native multi-wakeword detection daemon"
)]
struct Cli {
    /// Path to the TOML config file. Defaults to
    /// `$XDG_CONFIG_HOME/horchd/config.toml` (or `~/.config/horchd/config.toml`).
    #[arg(short, long, default_value_os_t = default_config_path())]
    config: PathBuf,

    /// Override the `RUST_LOG` env-filter for this run
    /// (e.g. `info`, `horchd=debug,zbus=warn`).
    #[arg(long)]
    log_level: Option<String>,
}

fn default_config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME").map_or_else(
        || {
            let home = std::env::var_os("HOME").unwrap_or_default();
            PathBuf::from(home).join(".config")
        },
        PathBuf::from,
    );
    base.join("horchd").join("config.toml")
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let filter = match cli.log_level.as_deref() {
        Some(level) => EnvFilter::new(level),
        None => EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    tracing::info!(config = %cli.config.display(), "loading config");
    let config = Config::load_from_file(&cli.config)
        .with_context(|| format!("loading config from {}", cli.config.display()))?;
    tracing::info!(wakewords = config.wakewords.len(), "config loaded");

    let preprocessor = inference::Preprocessor::new(
        &config.engine.shared_models.melspectrogram,
        &config.engine.shared_models.embedding,
    )
    .context("loading shared melspec + embedding models")?;
    let classifiers = config
        .wakewords
        .iter()
        .map(|w| inference::Classifier::load(w.name.clone(), &w.model))
        .collect::<Result<Vec<_>>>()
        .context("loading per-wakeword classifiers")?;
    tracing::info!(loaded = classifiers.len(), "classifiers loaded");

    let detectors: Vec<detector::Detector> = config
        .wakewords
        .iter()
        .map(|w| detector::Detector::new(w.name.clone(), w.threshold, w.cooldown_ms, w.enabled))
        .collect();
    let pipeline = inference::InferencePipeline::new(preprocessor, classifiers);

    let shared_state = state::DaemonState::new(config, cli.config.clone(), pipeline, detectors);

    let audio_stats = Arc::new(audio::AudioStats::new());
    let initial_device = {
        let s = shared_state.lock().await;
        s.config.engine.device.clone()
    };
    let (mut audio_handle, frames) =
        audio::start(&initial_device, AUDIO_CHANNEL_CAPACITY, Arc::clone(&audio_stats))
            .context("starting audio capture")?;
    let inference_stats = Arc::new(inference::InferenceStats::new());

    let (event_tx, _) = broadcast::channel::<WakewordEvent>(EVENT_BROADCAST_CAPACITY);
    let (score_tx, _) = broadcast::channel::<(String, f64)>(SCORE_BROADCAST_CAPACITY);
    let (audio_cmd_tx, mut audio_cmd_rx) = mpsc::channel::<AudioCmd>(8);

    let mut inference_handle = tokio::spawn(run_inference(
        frames,
        Arc::clone(&shared_state),
        event_tx.clone(),
        score_tx.clone(),
        Arc::clone(&inference_stats),
    ));
    tokio::spawn(log_stats(
        Arc::clone(&audio_stats),
        Arc::clone(&inference_stats),
        STATS_LOG_INTERVAL,
    ));

    let daemon = service::Daemon::new(
        Arc::clone(&shared_state),
        Arc::clone(&audio_stats),
        Arc::clone(&inference_stats),
        audio_cmd_tx.clone(),
    );
    let conn = zbus::connection::Builder::session()?
        .name(DBUS_NAME)?
        .serve_at(DBUS_PATH, daemon)?
        .build()
        .await
        .with_context(|| format!("registering {DBUS_NAME} on the session bus"))?;
    tracing::info!(
        service = DBUS_NAME,
        path = DBUS_PATH,
        "registered on session bus"
    );

    tokio::spawn(emit_signals(conn.clone(), event_tx.subscribe()));
    tokio::spawn(emit_score_snapshots(conn.clone(), score_tx.subscribe()));

    loop {
        tokio::select! {
            biased;
            _ = shutdown_signal() => break,
            maybe_cmd = audio_cmd_rx.recv() => {
                let Some(cmd) = maybe_cmd else { continue; };
                match cmd {
                    AudioCmd::List { reply } => {
                        let _ = reply.send(audio::list_input_device_names());
                    }
                    AudioCmd::SetDevice { name, persist, reply } => {
                        let res = swap_device(
                            &name,
                            persist,
                            &mut audio_handle,
                            &mut inference_handle,
                            &shared_state,
                            &audio_stats,
                            &inference_stats,
                            &event_tx,
                            &score_tx,
                        )
                        .await;
                        let _ = reply.send(res);
                    }
                }
            }
        }
    }
    drop(audio_handle);
    tracing::info!("shutdown");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn swap_device(
    name: &str,
    persist: bool,
    audio_handle: &mut audio::AudioHandle,
    inference_handle: &mut tokio::task::JoinHandle<()>,
    shared_state: &state::SharedState,
    audio_stats: &Arc<audio::AudioStats>,
    inference_stats: &Arc<inference::InferenceStats>,
    event_tx: &broadcast::Sender<WakewordEvent>,
    score_tx: &broadcast::Sender<(String, f64)>,
) -> Result<()> {
    tracing::info!(name, "switching audio input device");
    let (new_handle, frames) = audio::start(name, AUDIO_CHANNEL_CAPACITY, Arc::clone(audio_stats))
        .with_context(|| format!("opening audio device {name:?}"))?;

    inference_handle.abort();
    *audio_handle = new_handle; // drops old; cpal stream stops
    *inference_handle = tokio::spawn(run_inference(
        frames,
        Arc::clone(shared_state),
        event_tx.clone(),
        score_tx.clone(),
        Arc::clone(inference_stats),
    ));

    {
        let mut s = shared_state.lock().await;
        s.config.engine.device = name.to_owned();
    }
    if persist {
        let path = {
            let s = shared_state.lock().await;
            s.config_path.clone()
        };
        persist::set_engine_device(&path, name)
            .with_context(|| format!("persisting device {name:?} to {}", path.display()))?;
    }
    Ok(())
}

async fn run_inference(
    mut frames: tokio::sync::mpsc::Receiver<audio::Frame>,
    shared: state::SharedState,
    events: broadcast::Sender<WakewordEvent>,
    scores_tx: broadcast::Sender<(String, f64)>,
    stats: Arc<inference::InferenceStats>,
) {
    let mut last_snapshot: Option<Instant> = None;
    while let Some(frame) = frames.recv().await {
        let mut s = shared.lock().await;
        let result = tokio::task::block_in_place(|| s.pipeline.process(&frame));
        let scores = match result {
            Ok(scores) => scores,
            Err(err) => {
                tracing::error!(?err, "inference failed");
                continue;
            }
        };
        stats.record_score();

        let now = Instant::now();
        let snapshot_due = last_snapshot
            .map(|t| now.duration_since(t) >= SCORE_SNAPSHOT_INTERVAL)
            .unwrap_or(true);

        for (det, (name, score)) in s.detectors.iter_mut().zip(scores.iter()) {
            debug_assert_eq!(name, &det.name, "detector/classifier order mismatch");
            let score_f64 = f64::from(*score);
            if snapshot_due {
                let _ = scores_tx.send((name.clone(), score_f64));
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
            let _ = events.send(event);
        }
        if snapshot_due {
            last_snapshot = Some(now);
        }
    }
    tracing::warn!("audio frame channel closed");
}

async fn emit_signals(conn: Connection, mut events: broadcast::Receiver<WakewordEvent>) {
    use tokio::sync::broadcast::error::RecvError;
    loop {
        match events.recv().await {
            Ok(event) => emit_one(&conn, &event).await,
            Err(RecvError::Lagged(n)) => {
                tracing::warn!(skipped = n, "signal emitter lagged behind broadcast")
            }
            Err(RecvError::Closed) => {
                tracing::info!("event broadcast closed; signal emitter exiting");
                break;
            }
        }
    }
}

async fn emit_one(conn: &Connection, event: &WakewordEvent) {
    let emitter = match SignalEmitter::new(conn, DBUS_PATH) {
        Ok(e) => e,
        Err(err) => {
            tracing::error!(?err, "creating SignalEmitter");
            return;
        }
    };
    if let Err(err) =
        service::Daemon::detected(&emitter, &event.name, event.score, event.timestamp_us).await
    {
        tracing::error!(?err, name = %event.name, "emitting Detected signal");
    }
}

async fn emit_score_snapshots(conn: Connection, mut scores: broadcast::Receiver<(String, f64)>) {
    use tokio::sync::broadcast::error::RecvError;
    loop {
        match scores.recv().await {
            Ok((name, score)) => emit_score(&conn, &name, score).await,
            Err(RecvError::Lagged(n)) => {
                tracing::debug!(skipped = n, "score snapshot emitter lagged");
            }
            Err(RecvError::Closed) => {
                tracing::info!("score broadcast closed; snapshot emitter exiting");
                break;
            }
        }
    }
}

async fn emit_score(conn: &Connection, name: &str, score: f64) {
    let emitter = match SignalEmitter::new(conn, DBUS_PATH) {
        Ok(e) => e,
        Err(err) => {
            tracing::error!(?err, "creating SignalEmitter");
            return;
        }
    };
    if let Err(err) = service::Daemon::score_snapshot(&emitter, name, score).await {
        tracing::warn!(?err, name, "emitting ScoreSnapshot signal");
    }
}

async fn log_stats(
    audio: Arc<audio::AudioStats>,
    inference: Arc<inference::InferenceStats>,
    interval: Duration,
) {
    let mut tick = tokio::time::interval(interval);
    tick.tick().await;
    loop {
        tick.tick().await;
        tracing::debug!(
            audio_fps = format_args!("{:.2}", audio.audio_fps()),
            score_fps = format_args!("{:.2}", inference.score_fps()),
            audio_emitted = audio.frames_emitted(),
            audio_dropped = audio.frames_dropped(),
            scores = inference.scores_emitted(),
            "stats"
        );
    }
}

#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let sigterm = match signal(SignalKind::terminate()) {
        Ok(s) => Some(s),
        Err(err) => {
            tracing::warn!(?err, "could not install SIGTERM handler; SIGINT only");
            None
        }
    };
    let Some(mut sigterm) = sigterm else {
        tokio::signal::ctrl_c().await.ok();
        return;
    };
    tokio::select! {
        _ = sigterm.recv()          => tracing::info!(signal = "SIGTERM", "caught"),
        _ = tokio::signal::ctrl_c() => tracing::info!(signal = "SIGINT",  "caught"),
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.ok();
}
