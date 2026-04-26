use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use horchd_core::{Config, WakewordEvent};
use tokio::sync::broadcast;
use tracing_subscriber::EnvFilter;
use zbus::Connection;
use zbus::object_server::SignalEmitter;

mod audio;
mod detector;
mod inference;
mod service;

const DBUS_NAME: &str = "xyz.horchd.Daemon";
const DBUS_PATH: &str = "/xyz/horchd/Daemon";

/// Tokio mpsc capacity for raw audio frames.
/// 16 frames * 80 ms = 1.28 s of headroom against inference back-pressure.
const AUDIO_CHANNEL_CAPACITY: usize = 16;

/// Broadcast capacity for `Detected` events. 64 fires * cooldown is
/// minutes of headroom — slow subscribers won't lose anything realistic.
const EVENT_BROADCAST_CAPACITY: usize = 64;

/// Stats log cadence. Quiet enough for an always-on daemon; loud enough
/// that "is it running?" is one `journalctl --user -fu horchd` away.
const STATS_LOG_INTERVAL: Duration = Duration::from_secs(30);

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
    tracing::info!(
        loaded = classifiers.len(),
        names = ?classifiers.iter().map(|c| &c.name).collect::<Vec<_>>(),
        "classifiers loaded"
    );
    let detectors: Vec<detector::Detector> = config
        .wakewords
        .iter()
        .map(|w| detector::Detector::new(w.name.clone(), w.threshold, w.cooldown_ms, w.enabled))
        .collect();
    let pipeline = inference::InferencePipeline::new(preprocessor, classifiers);

    let (audio_handle, frames) = audio::start(&config.engine.device, AUDIO_CHANNEL_CAPACITY)
        .context("starting audio capture")?;
    let audio_stats = Arc::clone(&audio_handle.stats);
    let inference_stats = Arc::new(inference::InferenceStats::new());

    let (event_tx, _) = broadcast::channel::<WakewordEvent>(EVENT_BROADCAST_CAPACITY);

    tokio::spawn(run_inference(
        frames,
        pipeline,
        detectors,
        event_tx.clone(),
        Arc::clone(&inference_stats),
    ));
    tokio::spawn(log_stats(
        Arc::clone(&audio_stats),
        Arc::clone(&inference_stats),
        STATS_LOG_INTERVAL,
    ));

    let daemon = service::Daemon::new(
        config,
        cli.config.clone(),
        Arc::clone(&audio_stats),
        Arc::clone(&inference_stats),
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

    shutdown_signal().await;
    tracing::info!("shutdown");
    Ok(())
}

async fn run_inference(
    mut frames: tokio::sync::mpsc::Receiver<audio::Frame>,
    mut pipeline: inference::InferencePipeline,
    mut detectors: Vec<detector::Detector>,
    events: broadcast::Sender<WakewordEvent>,
    stats: Arc<inference::InferenceStats>,
) {
    while let Some(frame) = frames.recv().await {
        let result = tokio::task::block_in_place(|| pipeline.process(&frame));
        let scores = match result {
            Ok(scores) => scores,
            Err(err) => {
                tracing::error!(?err, "inference failed");
                continue;
            }
        };
        stats.record_score();

        let now = Instant::now();
        for (det, (name, score)) in detectors.iter_mut().zip(scores.iter()) {
            debug_assert_eq!(name, &det.name, "detector/classifier order mismatch");
            let Some(event) = det.update(f64::from(*score), now) else {
                continue;
            };
            tracing::info!(name = %event.name, score = event.score, ts_us = event.timestamp_us, "wakeword detected");
            // Fire-and-forget: send only fails if every receiver has been
            // dropped, which means no D-Bus subscriber cares right now.
            let _ = events.send(event);
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

async fn log_stats(
    audio: Arc<audio::AudioStats>,
    inference: Arc<inference::InferenceStats>,
    interval: Duration,
) {
    let mut tick = tokio::time::interval(interval);
    tick.tick().await; // skip immediate first tick
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
