//! Server-side `xyz.horchd.Daemon1` interface implementation.
//!
//! All in-memory state lives in [`DaemonState`] behind a single
//! `tokio::sync::Mutex`; the inference task and these methods take turns.
//! Method signatures must stay aligned with the proxy trait in
//! `horchd_core::dbus`.

use std::path::PathBuf;
use std::sync::Arc;

use horchd_core::{Config, Wakeword, WakewordSnapshot};
use zbus::interface;
use zbus::object_server::SignalEmitter;

use crate::audio::AudioStats;
use crate::detector::Detector;
use crate::inference::{Classifier, InferenceStats};
use crate::persist;
use crate::state::SharedState;

pub struct Daemon {
    state: SharedState,
    audio_stats: Arc<AudioStats>,
    inference_stats: Arc<InferenceStats>,
}

impl Daemon {
    pub fn new(
        state: SharedState,
        audio_stats: Arc<AudioStats>,
        inference_stats: Arc<InferenceStats>,
    ) -> Self {
        Self {
            state,
            audio_stats,
            inference_stats,
        }
    }
}

fn invalid_args(msg: impl Into<String>) -> zbus::fdo::Error {
    zbus::fdo::Error::InvalidArgs(msg.into())
}

fn failed(msg: impl Into<String>) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(msg.into())
}

fn snapshot(w: &Wakeword) -> WakewordSnapshot {
    (
        w.name.clone(),
        w.threshold,
        w.model.to_string_lossy().into_owned(),
        w.enabled,
        w.cooldown_ms,
    )
}

#[interface(name = "xyz.horchd.Daemon1")]
impl Daemon {
    /// Snapshot of the configured wakewords as
    /// `(name, threshold, model_path, enabled, cooldown_ms)`.
    async fn list_wakewords(&self) -> Vec<WakewordSnapshot> {
        self.state
            .lock()
            .await
            .config
            .wakewords
            .iter()
            .map(snapshot)
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

    /// Validate, load, and persist a new wakeword.
    async fn add(
        &self,
        name: &str,
        model_path: &str,
        threshold: f64,
        cooldown_ms: u32,
    ) -> zbus::fdo::Result<()> {
        if name.is_empty() {
            return Err(invalid_args("wakeword name must not be empty"));
        }
        let model = PathBuf::from(model_path);

        // Load classifier outside the lock so the lock is held only for
        // the in-memory mutation. `Classifier::load` is what validates
        // shape and produces an actionable error.
        let classifier = Classifier::load(name.to_owned(), &model)
            .map_err(|e| invalid_args(format!("loading model: {e:#}")))?;

        let mut s = self.state.lock().await;
        if s.config.wakewords.iter().any(|w| w.name == name) {
            return Err(invalid_args(format!("wakeword {name:?} already exists")));
        }

        let wake = Wakeword {
            name: name.to_owned(),
            model,
            threshold,
            cooldown_ms,
            enabled: Wakeword::DEFAULT_ENABLED,
        };

        persist::add_wakeword(&s.config_path, &wake).map_err(|e| failed(format!("{e:#}")))?;

        s.detectors.push(Detector::new(
            wake.name.clone(),
            wake.threshold,
            wake.cooldown_ms,
            wake.enabled,
        ));
        s.pipeline.add_classifier(classifier);
        s.config.wakewords.push(wake);
        tracing::info!(name, threshold, cooldown_ms, "wakeword added");
        Ok(())
    }

    /// Remove a wakeword. Does not delete the on-disk model.
    async fn remove(&self, name: &str) -> zbus::fdo::Result<()> {
        let mut s = self.state.lock().await;
        if !s.config.wakewords.iter().any(|w| w.name == name) {
            return Err(invalid_args(format!("unknown wakeword: {name}")));
        }
        persist::remove_wakeword(&s.config_path, name).map_err(|e| failed(format!("{e:#}")))?;
        s.config.wakewords.retain(|w| w.name != name);
        s.detectors.retain(|d| d.name != name);
        s.pipeline.remove_classifier(name);
        tracing::info!(name, "wakeword removed");
        Ok(())
    }

    async fn set_threshold(
        &self,
        name: &str,
        threshold: f64,
        persist_to_disk: bool,
    ) -> zbus::fdo::Result<()> {
        let mut s = self.state.lock().await;
        let det = s
            .detector_mut(name)
            .ok_or_else(|| invalid_args(format!("unknown wakeword: {name}")))?;
        det.threshold = threshold;
        if let Some(w) = s.wakeword_config_mut(name) {
            w.threshold = threshold;
        }
        if persist_to_disk {
            persist::set_threshold(&s.config_path, name, threshold)
                .map_err(|e| failed(format!("{e:#}")))?;
        }
        tracing::info!(name, threshold, persist_to_disk, "threshold updated");
        Ok(())
    }

    async fn set_enabled(
        &self,
        name: &str,
        enabled: bool,
        persist_to_disk: bool,
    ) -> zbus::fdo::Result<()> {
        let mut s = self.state.lock().await;
        let det = s
            .detector_mut(name)
            .ok_or_else(|| invalid_args(format!("unknown wakeword: {name}")))?;
        det.enabled = enabled;
        if let Some(w) = s.wakeword_config_mut(name) {
            w.enabled = enabled;
        }
        if persist_to_disk {
            persist::set_enabled(&s.config_path, name, enabled)
                .map_err(|e| failed(format!("{e:#}")))?;
        }
        tracing::info!(name, enabled, persist_to_disk, "enabled updated");
        Ok(())
    }

    async fn set_cooldown(
        &self,
        name: &str,
        ms: u32,
        persist_to_disk: bool,
    ) -> zbus::fdo::Result<()> {
        let mut s = self.state.lock().await;
        let det = s
            .detector_mut(name)
            .ok_or_else(|| invalid_args(format!("unknown wakeword: {name}")))?;
        det.cooldown = std::time::Duration::from_millis(u64::from(ms));
        if let Some(w) = s.wakeword_config_mut(name) {
            w.cooldown_ms = ms;
        }
        if persist_to_disk {
            persist::set_cooldown_ms(&s.config_path, name, ms)
                .map_err(|e| failed(format!("{e:#}")))?;
        }
        tracing::info!(name, cooldown_ms = ms, persist_to_disk, "cooldown updated");
        Ok(())
    }

    /// Re-read the config file from disk and reconcile in-memory state.
    /// New wakewords are loaded; removed ones are dropped; changed
    /// thresholds/cooldowns/enables are applied; entries whose model
    /// path changed are reloaded.
    async fn reload(&self) -> zbus::fdo::Result<()> {
        let mut s = self.state.lock().await;
        let new_config = Config::load_from_file(&s.config_path)
            .map_err(|e| failed(format!("reloading {}: {e:#}", s.config_path.display())))?;
        reconcile(&mut s, new_config).map_err(|e| failed(format!("{e:#}")))?;
        Ok(())
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

    /// Low-rate per-wakeword score snapshot (~5 Hz) for live UI meters.
    #[zbus(signal)]
    pub async fn score_snapshot(
        emitter: &SignalEmitter<'_>,
        name: &str,
        score: f64,
    ) -> zbus::Result<()>;
}

fn reconcile(state: &mut crate::state::DaemonState, new: Config) -> anyhow::Result<()> {
    use std::collections::HashMap;
    let old: HashMap<String, Wakeword> = state
        .config
        .wakewords
        .iter()
        .map(|w| (w.name.clone(), w.clone()))
        .collect();

    let mut added = Vec::new();
    let mut model_changed = Vec::new();
    let mut updated = Vec::new();
    let new_names: std::collections::HashSet<&str> =
        new.wakewords.iter().map(|w| w.name.as_str()).collect();

    for w in &new.wakewords {
        match old.get(&w.name) {
            None => added.push(w.clone()),
            Some(prev) if prev.model != w.model => model_changed.push(w.clone()),
            Some(_) => updated.push(w.clone()),
        }
    }
    let removed: Vec<String> = state
        .config
        .wakewords
        .iter()
        .filter(|w| !new_names.contains(w.name.as_str()))
        .map(|w| w.name.clone())
        .collect();

    for name in &removed {
        state.detectors.retain(|d| &d.name != name);
        state.pipeline.remove_classifier(name);
    }

    for w in &updated {
        if let Some(d) = state.detector_mut(&w.name) {
            d.threshold = w.threshold;
            d.cooldown = std::time::Duration::from_millis(u64::from(w.cooldown_ms));
            d.enabled = w.enabled;
        }
    }

    for w in model_changed.iter().chain(added.iter()) {
        let classifier = Classifier::load(w.name.clone(), &w.model)?;
        state.pipeline.remove_classifier(&w.name);
        state.pipeline.add_classifier(classifier);
        state.detectors.retain(|d| d.name != w.name);
        state.detectors.push(Detector::new(
            w.name.clone(),
            w.threshold,
            w.cooldown_ms,
            w.enabled,
        ));
    }

    state.config = new;
    tracing::info!(
        added = added.len(),
        removed = removed.len(),
        reloaded = model_changed.len(),
        updated = updated.len(),
        "reload reconciled"
    );
    Ok(())
}
