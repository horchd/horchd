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

pub struct Daemon {
    config: Arc<RwLock<Config>>,
    #[allow(dead_code)] // used by Reload / persist flows in later phases
    config_path: PathBuf,
}

impl Daemon {
    pub fn new(config: Config, config_path: PathBuf) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
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

    /// `(running, audio_fps, score_fps)`. Audio + inference land in
    /// later phases; the fps fields are zero until then.
    async fn get_status(&self) -> (bool, f64, f64) {
        (true, 0.0, 0.0)
    }
}
