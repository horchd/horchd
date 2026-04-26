use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use horchd_core::Config;
use tracing_subscriber::EnvFilter;

mod audio;
mod service;

const DBUS_NAME: &str = "xyz.horchd.Daemon";
const DBUS_PATH: &str = "/xyz/horchd/Daemon";

/// Tokio mpsc capacity for raw audio frames.
/// 16 frames * 80 ms = 1.28 s of headroom against inference back-pressure.
const AUDIO_CHANNEL_CAPACITY: usize = 16;

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

    let (audio_handle, frames) = audio::start(&config.engine.device, AUDIO_CHANNEL_CAPACITY)
        .context("starting audio capture")?;
    let stats = Arc::clone(&audio_handle.stats);

    tokio::spawn(drain_frames(frames));
    tokio::spawn(log_stats(Arc::clone(&stats), STATS_LOG_INTERVAL));

    let daemon = service::Daemon::new(config, cli.config.clone(), stats);
    let _conn = zbus::connection::Builder::session()?
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

    shutdown_signal().await;
    tracing::info!("shutdown");
    Ok(())
}

/// Phase 3 placeholder: discard each frame. Phase 4 replaces this with
/// the inference fan-out.
async fn drain_frames(mut frames: tokio::sync::mpsc::Receiver<audio::Frame>) {
    while frames.recv().await.is_some() {}
    tracing::warn!("audio frame channel closed");
}

async fn log_stats(stats: Arc<audio::AudioStats>, interval: Duration) {
    let mut tick = tokio::time::interval(interval);
    tick.tick().await; // skip immediate first tick
    loop {
        tick.tick().await;
        tracing::debug!(
            audio_fps = format_args!("{:.2}", stats.audio_fps()),
            emitted = stats.frames_emitted(),
            dropped = stats.frames_dropped(),
            "audio stats"
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
