//! Server-side `xyz.horchd.Daemon1` interface implementation.
//!
//! Method signatures must stay aligned with the proxy trait in
//! `horchd_core::dbus`. Methods not yet implemented for the current phase
//! are simply absent — calling them yields `UnknownMethod`.

use std::path::PathBuf;
use std::sync::Arc;

use horchd_core::{Config, WakewordSnapshot};
use tokio::sync::RwLock;
use zbus::interface;
use zbus::object_server::SignalEmitter;

use crate::audio::AudioStats;
use crate::inference::InferenceStats;

pub struct Daemon {
    config: Arc<RwLock<Config>>,
    audio_stats: Arc<AudioStats>,
    inference_stats: Arc<InferenceStats>,
    #[allow(dead_code)] // used by Reload / persist flows in later phases
    config_path: PathBuf,
}

impl Daemon {
    pub fn new(
        config: Config,
        config_path: PathBuf,
        audio_stats: Arc<AudioStats>,
        inference_stats: Arc<InferenceStats>,
    ) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            audio_stats,
            inference_stats,
            config_path,
        }
    }
}

#[interface(name = "xyz.horchd.Daemon1")]
impl Daemon {
    /// Snapshot of the configured wakewords as
    /// `(name, threshold, model_path, enabled, cooldown_ms)`.
    async fn list_wakewords(&self) -> Vec<WakewordSnapshot> {
        let cfg = self.config.read().await;
        cfg.wakewords
            .iter()
            .map(|w| {
                (
                    w.name.clone(),
                    w.threshold,
                    w.model.to_string_lossy().into_owned(),
                    w.enabled,
                    w.cooldown_ms,
                )
            })
            .collect()
    }

    /// `(running, audio_fps, score_fps)`.
    async fn get_status(&self) -> (bool, f64, f64) {
        (
            true,
            self.audio_stats.audio_fps(),
            self.inference_stats.score_fps(),
        )
    }

    /// Emitted on the rising edge when a wakeword's score crosses its
    /// threshold for the first time within a cooldown window. Subscribers
    /// receive `(name, score, timestamp_us)`.
    #[zbus(signal)]
    pub async fn detected(
        emitter: &SignalEmitter<'_>,
        name: &str,
        score: f64,
        timestamp_us: u64,
    ) -> zbus::Result<()>;
}
