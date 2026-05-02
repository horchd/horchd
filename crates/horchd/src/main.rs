use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use horchd::audio::{AudioStats, MicSource};
use horchd::pipeline::Pipeline;
use horchd::sink::DBusSink;
use horchd::{AudioCmd, audio, detector, inference, persist, service, state};
use horchd_client::{AudioSource, Config, DetectionSink};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing_subscriber::EnvFilter;

const DBUS_NAME: &str = "xyz.horchd.Daemon";
const DBUS_PATH: &str = "/xyz/horchd/Daemon";

/// 16 frames * 80 ms = 1.28 s of headroom against inference back-pressure.
const AUDIO_CHANNEL_CAPACITY: usize = 16;

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
    tracing::info!(loaded = classifiers.len(), "classifiers loaded");

    let detectors: Vec<detector::Detector> = config
        .wakewords
        .iter()
        .map(|w| detector::Detector::new(w.name.clone(), w.threshold, w.cooldown_ms, w.enabled))
        .collect();
    let inference_pipeline = inference::InferencePipeline::new(preprocessor, classifiers);

    let shared_state =
        state::DaemonState::new(config, cli.config.clone(), inference_pipeline, detectors);
    let audio_stats = Arc::new(AudioStats::new());
    let inference_stats = Arc::new(inference::InferenceStats::new());

    let (audio_cmd_tx, mut audio_cmd_rx) = mpsc::channel::<AudioCmd>(8);
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

    let pipeline = Arc::new(Pipeline::new(
        Arc::clone(&shared_state),
        Arc::clone(&inference_stats),
    ));
    // Subscribe BEFORE the first source starts: events fired before a
    // sink subscribes are dropped.
    let dbus_sink: Arc<dyn DetectionSink> = Arc::new(DBusSink::new(conn.clone()));
    let _dbus_handle = pipeline.add_sink(dbus_sink);

    let initial_device = shared_state.lock().await.config.engine.device.clone();
    let mut mic = MicSource::new(
        initial_device,
        AUDIO_CHANNEL_CAPACITY,
        Arc::clone(&audio_stats),
    );
    let frames = mic.start().context("starting audio capture")?;
    let mut inference_handle = spawn_inference(Arc::clone(&pipeline), frames);

    tokio::spawn(log_stats(
        Arc::clone(&audio_stats),
        Arc::clone(&inference_stats),
        STATS_LOG_INTERVAL,
    ));

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
                            &mut mic,
                            &mut inference_handle,
                            &shared_state,
                            &audio_stats,
                            &pipeline,
                        )
                        .await;
                        let _ = reply.send(res);
                    }
                }
            }
        }
    }
    drop(mic);
    tracing::info!("shutdown");
    Ok(())
}

fn spawn_inference(
    pipeline: Arc<Pipeline>,
    frames: mpsc::Receiver<horchd_client::AudioFrame>,
) -> JoinHandle<()> {
    tokio::spawn(async move { pipeline.run(frames).await })
}

#[allow(clippy::too_many_arguments)]
async fn swap_device(
    name: &str,
    persist: bool,
    mic: &mut MicSource,
    inference_handle: &mut JoinHandle<()>,
    shared_state: &state::SharedState,
    audio_stats: &Arc<AudioStats>,
    pipeline: &Arc<Pipeline>,
) -> Result<()> {
    tracing::info!(name, "switching audio input device");
    let mut new_mic = MicSource::new(
        name.to_string(),
        AUDIO_CHANNEL_CAPACITY,
        Arc::clone(audio_stats),
    );
    // Open the new device BEFORE tearing down the old one. If this
    // fails the running mic stays untouched.
    let frames = new_mic
        .start()
        .with_context(|| format!("opening audio device {name:?}"))?;

    inference_handle.abort();
    *mic = new_mic; // drops old; cpal stream stops
    *inference_handle = spawn_inference(Arc::clone(pipeline), frames);

    {
        let mut s = shared_state.lock().await;
        name.clone_into(&mut s.config.engine.device);
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

async fn log_stats(
    audio: Arc<AudioStats>,
    inference: Arc<inference::InferenceStats>,
    interval: Duration,
) {
    let mut tick = tokio::time::interval(interval);
    tick.tick().await;
    loop {
        tick.tick().await;
        tracing::info!(
            audio_fps = format_args!("{:.2}", audio.audio_fps()),
            score_fps = format_args!("{:.2}", inference.score_fps()),
            audio_emitted = audio.frames_emitted(),
            audio_dropped = audio.frames_dropped(),
            scores = inference.scores_emitted(),
            mean_latency_us = inference.mean_latency_us(),
            max_latency_us = inference.max_latency_us(),
            last_latency_us = inference.last_latency_us(),
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
