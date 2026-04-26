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

use crate::audio::AudioStats;

pub struct Daemon {
    config: Arc<RwLock<Config>>,
    audio_stats: Arc<AudioStats>,
    #[allow(dead_code)] // used by Reload / persist flows in later phases
    config_path: PathBuf,
}

impl Daemon {
    pub fn new(config: Config, config_path: PathBuf, audio_stats: Arc<AudioStats>) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            audio_stats,
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

    /// `(running, audio_fps, score_fps)`. Score-fps stays at zero until
    /// the inference pipeline lands in Phase 4.
    async fn get_status(&self) -> (bool, f64, f64) {
        (true, self.audio_stats.audio_fps(), 0.0)
    }
}
