//! Server-side `xyz.horchd.Daemon1` interface implementation.
//!
//! All in-memory state lives in [`DaemonState`] behind a single
//! `tokio::sync::Mutex`; the inference task and these methods take turns.
//! Method signatures must stay aligned with the proxy trait in
//! `horchd_core::dbus`.
//!
//! Heavy work (ONNX `Session` construction, TOML reload + reconcile) is
//! moved off the runtime thread via `spawn_blocking` and the lock is
//! dropped while it runs, so a 200-ms model load doesn't stall every
//! D-Bus method on the bus.

use std::path::PathBuf;
use std::sync::Arc;

use horchd_core::{Config, MAX_COOLDOWN_MS, Wakeword, WakewordSnapshot};
use tokio::sync::{mpsc, oneshot};
use zbus::interface;
use zbus::object_server::SignalEmitter;

use crate::AudioCmd;
use crate::audio::AudioStats;
use crate::detector::Detector;
use crate::inference::{Classifier, InferenceStats};
use crate::persist;
use crate::state::SharedState;

pub struct Daemon {
    state: SharedState,
    audio_stats: Arc<AudioStats>,
    inference_stats: Arc<InferenceStats>,
    audio_cmd_tx: mpsc::Sender<AudioCmd>,
}

impl Daemon {
    pub fn new(
        state: SharedState,
        audio_stats: Arc<AudioStats>,
        inference_stats: Arc<InferenceStats>,
        audio_cmd_tx: mpsc::Sender<AudioCmd>,
    ) -> Self {
        Self {
            state,
            audio_stats,
            inference_stats,
            audio_cmd_tx,
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

fn validate_name(name: &str) -> Result<(), zbus::fdo::Error> {
    if name.is_empty() {
        return Err(invalid_args("wakeword name must not be empty"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(invalid_args(format!(
            "wakeword name {name:?} must only use ASCII letters, digits, '_' or '-'"
        )));
    }
    if name.starts_with('-') {
        return Err(invalid_args("wakeword name must not start with '-'"));
    }
    Ok(())
}

fn validate_threshold(value: f64) -> Result<(), zbus::fdo::Error> {
    if !(value > 0.0 && value <= 1.0) {
        return Err(invalid_args(format!(
            "threshold must be in (0, 1]; got {value}"
        )));
    }
    Ok(())
}

fn validate_cooldown(ms: u32) -> Result<(), zbus::fdo::Error> {
    if ms > MAX_COOLDOWN_MS {
        return Err(invalid_args(format!(
            "cooldown_ms must be ≤ {MAX_COOLDOWN_MS} (got {ms})"
        )));
    }
    Ok(())
}

/// Resolve `model_path` to an absolute `PathBuf` and refuse to register
/// anything outside the canonical models directory
/// (`$XDG_DATA_HOME/horchd/models/` or `~/.local/share/horchd/models/`).
/// This is the boundary that prevents an unprivileged session-bus client
/// from convincing the daemon to try and load `/etc/passwd` as a model.
fn resolve_model_path(model_path: &str) -> Result<PathBuf, zbus::fdo::Error> {
    let raw = shellexpand::tilde(model_path).into_owned();
    let path = PathBuf::from(&raw);
    if !path.is_absolute() {
        return Err(invalid_args(format!(
            "model path must be absolute; got {model_path:?}"
        )));
    }
    let root = canonical_models_dir().map_err(|e| failed(format!("models dir: {e:#}")))?;
    let canonical = path
        .canonicalize()
        .map_err(|e| invalid_args(format!("model file {raw}: {e}")))?;
    let canonical_root = root.canonicalize().unwrap_or(root);
    if !canonical.starts_with(&canonical_root) {
        return Err(invalid_args(format!(
            "model path must live under {} (got {})",
            canonical_root.display(),
            canonical.display()
        )));
    }
    Ok(canonical)
}

fn canonical_models_dir() -> std::io::Result<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .ok_or_else(|| std::io::Error::other("$HOME / $XDG_DATA_HOME not set"))?;
    Ok(base.join("horchd").join("models"))
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

    /// `(running, audio_fps, score_fps, mic_level)`. `mic_level` is the
    /// smoothed peak |sample| of the most recent cpal callback, in `[0, 1]`.
    async fn get_status(&self) -> (bool, f64, f64, f64) {
        (
            true,
            self.audio_stats.audio_fps(),
            self.inference_stats.score_fps(),
            f64::from(self.audio_stats.last_peak()),
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
        validate_name(name)?;
        validate_threshold(threshold)?;
        validate_cooldown(cooldown_ms)?;
        let model = resolve_model_path(model_path)?;

        // Move the heavy ORT session construction off the runtime thread
        // and OUT of the state lock. `Classifier::load` validates the
        // shape and returns an actionable error.
        let model_for_load = model.clone();
        let name_for_load = name.to_owned();
        let classifier =
            tokio::task::spawn_blocking(move || Classifier::load(name_for_load, &model_for_load))
                .await
                .map_err(|e| failed(format!("classifier load join error: {e}")))?
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
        validate_threshold(threshold)?;
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
        validate_cooldown(ms)?;
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

    /// Sorted list of cpal input device names available on the default
    /// host. Cheap — only enumerates, does not open any streams.
    async fn list_input_devices(&self) -> zbus::fdo::Result<Vec<String>> {
        let (tx, rx) = oneshot::channel();
        self.audio_cmd_tx
            .send(AudioCmd::List { reply: tx })
            .await
            .map_err(|_| failed("audio command channel closed"))?;
        let res = rx
            .await
            .map_err(|_| failed("audio task dropped reply channel"))?;
        res.map_err(|e| failed(format!("{e:#}")))
    }

    /// Hot-swap the cpal capture device. Drops the running stream,
    /// starts a new one, restarts the inference task. `"default"`
    /// follows the host default. `persist=true` writes the choice back
    /// to `[engine].device` in `config.toml`.
    async fn set_input_device(&self, name: &str, persist_to_disk: bool) -> zbus::fdo::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.audio_cmd_tx
            .send(AudioCmd::SetDevice {
                name: name.to_owned(),
                persist: persist_to_disk,
                reply: tx,
            })
            .await
            .map_err(|_| failed("audio command channel closed"))?;
        let res = rx
            .await
            .map_err(|_| failed("audio task dropped reply channel"))?;
        res.map_err(|e| failed(format!("{e:#}")))
    }

    /// Re-read the config file from disk and reconcile in-memory state.
    /// New wakewords are loaded; removed ones are dropped; changed
    /// thresholds/cooldowns/enables are applied; entries whose model
    /// path changed are reloaded.
    ///
    /// Heavy I/O (file read, classifier loads) runs OUTSIDE the lock; we
    /// only take the lock to swap fully-constructed pieces in. This keeps
    /// audio frame processing flowing during a reload.
    async fn reload(&self) -> zbus::fdo::Result<()> {
        let config_path = {
            let s = self.state.lock().await;
            s.config_path.clone()
        };
        let new_config = tokio::task::spawn_blocking({
            let p = config_path.clone();
            move || Config::load_from_file(&p)
        })
        .await
        .map_err(|e| failed(format!("reload join error: {e}")))?
        .map_err(|e| failed(format!("reloading {}: {e:#}", config_path.display())))?;

        // Build the change-set off-lock.
        let plan = {
            let s = self.state.lock().await;
            build_reload_plan(&s.config, &new_config)
        };

        // Load every (added or model-changed) classifier off-lock.
        let to_load: Vec<(String, PathBuf)> = plan
            .added
            .iter()
            .chain(plan.model_changed.iter())
            .map(|w| (w.name.clone(), w.model.clone()))
            .collect();
        let loaded = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<Classifier>> {
            to_load
                .into_iter()
                .map(|(n, p)| Classifier::load(n, &p))
                .collect()
        })
        .await
        .map_err(|e| failed(format!("classifier load join error: {e}")))?
        .map_err(|e| failed(format!("loading classifiers: {e:#}")))?;

        // Now apply in one short critical section.
        let mut s = self.state.lock().await;
        apply_reload_plan(&mut s, plan, loaded, new_config);
        Ok(())
    }

    /// Emitted on the rising edge when a wakeword's score crosses its
    /// threshold for the first time within a cooldown window.
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

struct ReloadPlan {
    added: Vec<Wakeword>,
    model_changed: Vec<Wakeword>,
    updated: Vec<Wakeword>,
    removed: Vec<String>,
}

fn build_reload_plan(old: &Config, new: &Config) -> ReloadPlan {
    use std::collections::{HashMap, HashSet};
    let prev: HashMap<&str, &Wakeword> =
        old.wakewords.iter().map(|w| (w.name.as_str(), w)).collect();
    let new_names: HashSet<&str> = new.wakewords.iter().map(|w| w.name.as_str()).collect();

    let mut added = Vec::new();
    let mut model_changed = Vec::new();
    let mut updated = Vec::new();
    for w in &new.wakewords {
        match prev.get(w.name.as_str()) {
            None => added.push(w.clone()),
            Some(p) if p.model != w.model => model_changed.push(w.clone()),
            Some(_) => updated.push(w.clone()),
        }
    }
    let removed: Vec<String> = old
        .wakewords
        .iter()
        .filter(|w| !new_names.contains(w.name.as_str()))
        .map(|w| w.name.clone())
        .collect();
    ReloadPlan {
        added,
        model_changed,
        updated,
        removed,
    }
}

fn apply_reload_plan(
    state: &mut crate::state::DaemonState,
    plan: ReloadPlan,
    mut loaded: Vec<Classifier>,
    new_config: Config,
) {
    for name in &plan.removed {
        state.detectors.retain(|d| &d.name != name);
        state.pipeline.remove_classifier(name);
    }
    for w in &plan.updated {
        if let Some(d) = state.detector_mut(&w.name) {
            d.threshold = w.threshold;
            d.cooldown = std::time::Duration::from_millis(u64::from(w.cooldown_ms));
            d.enabled = w.enabled;
        }
    }
    for w in plan.model_changed.iter().chain(plan.added.iter()) {
        // `loaded` is in (added ++ model_changed) order from the call
        // site, but we assigned it in the same iteration — pull from the
        // front so name-by-name pairing is preserved.
        let Some(classifier) = loaded
            .iter()
            .position(|c| c.name == w.name)
            .map(|i| loaded.remove(i))
        else {
            tracing::warn!(name = %w.name, "missing classifier in reload plan; skipping");
            continue;
        };
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
    state.config = new_config;
    tracing::info!(
        added = plan.added.len(),
        removed = plan.removed.len(),
        reloaded = plan.model_changed.len(),
        updated = plan.updated.len(),
        "reload reconciled"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use horchd_core::{Engine, SharedModels};

    fn engine_for_tests() -> Engine {
        Engine {
            device: "default".into(),
            sample_rate: 16_000,
            log_level: "info".into(),
            shared_models: SharedModels {
                melspectrogram: PathBuf::from("/m.onnx"),
                embedding: PathBuf::from("/e.onnx"),
            },
        }
    }

    fn cfg_with(wakes: Vec<Wakeword>) -> Config {
        Config {
            engine: engine_for_tests(),
            wakewords: wakes,
        }
    }

    fn wake(name: &str, model: &str, threshold: f64) -> Wakeword {
        Wakeword {
            name: name.into(),
            model: PathBuf::from(model),
            threshold,
            cooldown_ms: 1500,
            enabled: true,
        }
    }

    #[test]
    fn validate_name_accepts_normal_names() {
        validate_name("alexa").unwrap();
        validate_name("hey_jarvis_v0_1").unwrap();
        validate_name("foo-bar").unwrap();
    }

    #[test]
    fn validate_name_rejects_traversal_and_separators() {
        for bad in ["", "../foo", "foo/bar", "foo bar", "foo\0bar", "foo.onnx"] {
            assert!(validate_name(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn validate_name_rejects_dash_prefix() {
        assert!(validate_name("-rm").is_err());
    }

    #[test]
    fn validate_threshold_range() {
        assert!(validate_threshold(0.5).is_ok());
        assert!(validate_threshold(1.0).is_ok());
        assert!(validate_threshold(0.0).is_err());
        assert!(validate_threshold(-0.1).is_err());
        assert!(validate_threshold(1.5).is_err());
        assert!(validate_threshold(f64::NAN).is_err());
    }

    #[test]
    fn validate_cooldown_cap() {
        assert!(validate_cooldown(0).is_ok());
        assert!(validate_cooldown(MAX_COOLDOWN_MS).is_ok());
        assert!(validate_cooldown(MAX_COOLDOWN_MS + 1).is_err());
    }

    #[test]
    fn build_reload_plan_classifies_changes() {
        let old = cfg_with(vec![wake("a", "/a.onnx", 0.5), wake("b", "/b.onnx", 0.5)]);
        let new = cfg_with(vec![
            wake("a", "/a-new.onnx", 0.5), // model changed
            wake("b", "/b.onnx", 0.7),     // threshold changed
            wake("c", "/c.onnx", 0.5),     // added
        ]);
        let plan = build_reload_plan(&old, &new);
        assert_eq!(
            plan.added.iter().map(|w| &w.name).collect::<Vec<_>>(),
            vec!["c"]
        );
        assert_eq!(
            plan.model_changed
                .iter()
                .map(|w| &w.name)
                .collect::<Vec<_>>(),
            vec!["a"]
        );
        assert_eq!(
            plan.updated.iter().map(|w| &w.name).collect::<Vec<_>>(),
            vec!["b"]
        );
        assert!(plan.removed.is_empty());
    }

    #[test]
    fn build_reload_plan_detects_removals() {
        let old = cfg_with(vec![wake("a", "/a.onnx", 0.5), wake("b", "/b.onnx", 0.5)]);
        let new = cfg_with(vec![wake("a", "/a.onnx", 0.5)]);
        let plan = build_reload_plan(&old, &new);
        assert_eq!(plan.removed, vec!["b"]);
        assert!(plan.added.is_empty());
        assert!(plan.model_changed.is_empty());
        assert_eq!(
            plan.updated.iter().map(|w| &w.name).collect::<Vec<_>>(),
            vec!["a"]
        );
    }
}
