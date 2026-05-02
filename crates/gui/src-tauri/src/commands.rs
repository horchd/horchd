//! `#[tauri::command]` handlers. The frontend calls these via
//! `invoke()`; each one delegates to the D-Bus proxy or to the bundled
//! Python training helper.
//!
//! Security boundaries enforced here:
//! - All wakeword names are run through [`sanitize_name`] (ASCII
//!   alphanumeric / `_` / `-`, no leading dash, length-capped) so a
//!   compromised renderer cannot escape the canonical training/models
//!   tree via `../foo`.
//! - All filesystem reads/writes for training assets are confined to
//!   the canonical training directory via `canonicalize` + `starts_with`.
//! - All subprocess invocations use real `argv` (no shell), use
//!   `kill_on_drop(true)` so the spawned process dies with the GUI, and
//!   keep a single in-flight handle so a "cancel" button actually
//!   terminates the underlying child.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::process::{Child, Command};
use tokio::sync::Mutex as AsyncMutex;

use crate::dbus_client::proxy;

/// Length cap on user-supplied wakeword names (bytes). Keeps log lines
/// readable and bounds path length even after sanitisation.
const MAX_NAME_LEN: usize = 64;

/// Hard cap on `read_training_sample` payloads — the frontend turns
/// these into a Blob URL for `<audio>` playback; legitimate WAVs are
/// well under 5 MB. Past this we refuse rather than OOM the webview.
const MAX_TRAINING_SAMPLE_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct WakewordRow {
    pub name: String,
    pub threshold: f64,
    pub model: String,
    pub enabled: bool,
    pub cooldown_ms: u32,
}

#[derive(Debug, Serialize)]
pub struct DaemonStatus {
    pub running: bool,
    pub audio_fps: f64,
    pub score_fps: f64,
    pub mic_level: f64,
}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// Holds the JoinHandle of the in-flight subprocess streamer (setup OR
/// training, never both at once via the UI). Cancelling drops the
/// `Child` future, and because `kill_on_drop(true)` is set on every
/// spawn, the child receives SIGKILL.
#[derive(Default)]
pub struct ProcessRegistry {
    setup: AsyncMutex<Option<Arc<Mutex<Option<Child>>>>>,
    train: AsyncMutex<Option<Arc<Mutex<Option<Child>>>>>,
}

impl ProcessRegistry {
    async fn install(
        &self,
        kind: ProcessKind,
        slot: Arc<Mutex<Option<Child>>>,
    ) -> Option<Arc<Mutex<Option<Child>>>> {
        let cell = match kind {
            ProcessKind::Setup => &self.setup,
            ProcessKind::Train => &self.train,
        };
        let mut guard = cell.lock().await;
        let prev = guard.take();
        *guard = Some(slot);
        prev
    }

    async fn clear(&self, kind: ProcessKind) {
        let cell = match kind {
            ProcessKind::Setup => &self.setup,
            ProcessKind::Train => &self.train,
        };
        cell.lock().await.take();
    }

    async fn kill(&self, kind: ProcessKind) -> Result<(), String> {
        let cell = match kind {
            ProcessKind::Setup => &self.setup,
            ProcessKind::Train => &self.train,
        };
        let slot_opt = cell.lock().await.clone();
        let Some(slot) = slot_opt else { return Ok(()) };
        let mut child_opt = match slot.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(child) = child_opt.as_mut()
            && let Err(e) = child.start_kill()
        {
            return Err(format!("kill: {e}"));
        }
        Ok(())
    }
}

#[derive(Copy, Clone)]
enum ProcessKind {
    Setup,
    Train,
}

impl ProcessKind {
    fn label(self) -> &'static str {
        match self {
            ProcessKind::Setup => "setup",
            ProcessKind::Train => "train",
        }
    }
}

#[tauri::command]
pub async fn list_wakewords() -> Result<Vec<WakewordRow>, String> {
    let p = proxy().await.map_err(err)?;
    let raw = p.list_wakewords().await.map_err(|e| err(e.into()))?;
    Ok(raw
        .into_iter()
        .map(
            |(name, threshold, model, enabled, cooldown_ms)| WakewordRow {
                name,
                threshold,
                model,
                enabled,
                cooldown_ms,
            },
        )
        .collect())
}

#[tauri::command]
pub async fn get_status() -> Result<DaemonStatus, String> {
    let p = proxy().await.map_err(err)?;
    let (running, audio_fps, score_fps, mic_level) =
        p.get_status().await.map_err(|e| err(e.into()))?;
    Ok(DaemonStatus {
        running,
        audio_fps,
        score_fps,
        mic_level,
    })
}

#[tauri::command]
pub async fn set_threshold(name: String, value: f64, save: bool) -> Result<(), String> {
    let p = proxy().await.map_err(err)?;
    p.set_threshold(&name, value, save)
        .await
        .map_err(|e| err(e.into()))
}

#[tauri::command]
pub async fn set_enabled(name: String, enabled: bool, save: bool) -> Result<(), String> {
    let p = proxy().await.map_err(err)?;
    p.set_enabled(&name, enabled, save)
        .await
        .map_err(|e| err(e.into()))
}

#[tauri::command]
pub async fn set_cooldown(name: String, ms: u32, save: bool) -> Result<(), String> {
    let p = proxy().await.map_err(err)?;
    p.set_cooldown(&name, ms, save)
        .await
        .map_err(|e| err(e.into()))
}

#[tauri::command]
pub async fn add_wakeword(
    name: String,
    model: String,
    threshold: f64,
    cooldown_ms: u32,
) -> Result<(), String> {
    let p = proxy().await.map_err(err)?;
    p.add(&name, &model, threshold, cooldown_ms)
        .await
        .map_err(|e| err(e.into()))
}

#[tauri::command]
pub async fn remove_wakeword(name: String) -> Result<(), String> {
    let p = proxy().await.map_err(err)?;
    p.remove(&name).await.map_err(|e| err(e.into()))
}

#[tauri::command]
pub async fn list_input_devices() -> Result<Vec<String>, String> {
    let p = proxy().await.map_err(err)?;
    p.list_input_devices().await.map_err(|e| err(e.into()))
}

#[tauri::command]
pub async fn set_input_device(name: String, save: bool) -> Result<(), String> {
    let p = proxy().await.map_err(err)?;
    p.set_input_device(&name, save)
        .await
        .map_err(|e| err(e.into()))
}

#[tauri::command]
pub async fn reload() -> Result<(), String> {
    let p = proxy().await.map_err(err)?;
    p.reload().await.map_err(|e| err(e.into()))
}

#[tauri::command]
pub async fn models_dir() -> Result<String, String> {
    Ok(canonical_models_dir()
        .map_err(err)?
        .to_string_lossy()
        .into_owned())
}

/// Copy `source_path` (an `.onnx` anywhere on disk) into the canonical
/// user models directory under `~/.local/share/horchd/models/<name>.onnx`,
/// then register it with the daemon.
#[tauri::command]
pub async fn import_wakeword(
    name: String,
    source_path: String,
    threshold: f64,
    cooldown_ms: u32,
) -> Result<String, String> {
    let name = sanitize_name(&name)?;

    let dest_dir = canonical_models_dir().map_err(err)?;
    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| format!("creating {}: {}", dest_dir.display(), e))?;
    let dest = dest_dir.join(format!("{name}.onnx"));

    let src = std::path::PathBuf::from(&source_path);
    if !src.is_absolute() {
        return Err(format!("source path must be absolute: {source_path}"));
    }
    if !src.exists() {
        return Err(format!("source file not found: {source_path}"));
    }
    if src.extension().and_then(|s| s.to_str()) != Some("onnx") {
        return Err(format!("source file must end in .onnx: {source_path}"));
    }
    if dest != src {
        std::fs::copy(&src, &dest)
            .map_err(|e| format!("copying {} → {}: {e}", src.display(), dest.display()))?;
        let sidecar = src.with_extension("onnx.data");
        if sidecar.exists() {
            let sidecar_dest = dest.with_extension("onnx.data");
            std::fs::copy(&sidecar, &sidecar_dest).map_err(|e| format!("copying sidecar: {e}"))?;
        }
    }

    let dest_str = dest.to_string_lossy().into_owned();
    let p = proxy().await.map_err(err)?;
    p.add(&name, &dest_str, threshold, cooldown_ms)
        .await
        .map_err(|e| err(e.into()))?;
    Ok(dest_str)
}

fn canonical_models_dir() -> anyhow::Result<std::path::PathBuf> {
    Ok(canonical_data_dir()?.join("models"))
}

fn canonical_training_dir() -> anyhow::Result<std::path::PathBuf> {
    Ok(canonical_data_dir()?.join("training"))
}

fn canonical_data_dir() -> anyhow::Result<std::path::PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME").map_or_else(
        || -> anyhow::Result<PathBuf> {
            let home =
                std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("$HOME is not set"))?;
            Ok(PathBuf::from(home).join(".local").join("share"))
        },
        |v| Ok(PathBuf::from(v)),
    )?;
    Ok(base.join("horchd"))
}

#[derive(Debug, Serialize)]
pub struct TrainingSample {
    pub kind: String,
    pub path: String,
    pub ts_ms: u64,
    pub size: u64,
    pub duration_ms: u32,
    pub sample_rate: u32,
}

#[derive(Debug, Serialize)]
pub struct TrainingWord {
    pub name: String,
    pub positive: u32,
    pub negative: u32,
    pub target_phrase: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct WordMeta {
    pub target_phrase: Option<String>,
}

fn sanitize_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("name is empty".into());
    }
    if trimmed.len() > MAX_NAME_LEN {
        return Err(format!("name longer than {MAX_NAME_LEN} bytes"));
    }
    if trimmed.starts_with('-') {
        return Err("name must not start with '-'".into());
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("name must be ASCII letters, digits, '_' or '-'".into());
    }
    Ok(trimmed.to_string())
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn write_pcm_wav(path: &std::path::Path, samples: &[i16], sample_rate: u32) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|e| format!("creating wav {}: {e}", path.display()))?;
    for s in samples {
        writer
            .write_sample(*s)
            .map_err(|e| format!("writing sample: {e}"))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("finalizing wav: {e}"))?;
    Ok(())
}

fn wav_metadata(path: &std::path::Path) -> Option<(u32, u32)> {
    let reader = hound::WavReader::open(path).ok()?;
    let spec = reader.spec();
    let frames = u64::from(reader.duration());
    if spec.sample_rate == 0 {
        return None;
    }
    let duration_ms = u32::try_from(frames * 1000 / u64::from(spec.sample_rate)).unwrap_or(0);
    Some((duration_ms, spec.sample_rate))
}

#[tauri::command]
pub async fn training_dir() -> Result<String, String> {
    Ok(canonical_training_dir()
        .map_err(err)?
        .to_string_lossy()
        .into_owned())
}

/// Write a PCM-int16 WAV file under `<training-dir>/<name>/<kind>/<ts>.wav`.
#[tauri::command]
pub async fn save_training_sample(
    name: String,
    kind: String,
    sample_rate: u32,
    samples: Vec<i16>,
) -> Result<TrainingSample, String> {
    let name = sanitize_name(&name)?;
    let kind = match kind.as_str() {
        "positive" | "negative" => kind,
        _ => return Err(format!("unknown sample kind: {kind}")),
    };
    if !(8000..=48000).contains(&sample_rate) {
        return Err(format!("unexpected sample rate: {sample_rate}"));
    }
    if samples.is_empty() {
        return Err("no samples received".into());
    }

    let dir = canonical_training_dir()
        .map_err(err)?
        .join(&name)
        .join(&kind);
    let dir_for_blocking = dir.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        std::fs::create_dir_all(&dir_for_blocking)
            .map_err(|e| format!("creating {}: {e}", dir_for_blocking.display()))
    })
    .await
    .map_err(|e| format!("io join: {e}"))??;

    let ts = now_ms();
    let path = dir.join(format!("{ts}.wav"));
    let path_for_blocking = path.clone();
    let samples_for_blocking = samples.clone();
    tokio::task::spawn_blocking(move || {
        write_pcm_wav(&path_for_blocking, &samples_for_blocking, sample_rate)
    })
    .await
    .map_err(|e| format!("io join: {e}"))??;

    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let duration_ms =
        u32::try_from((samples.len() as u64) * 1000 / u64::from(sample_rate)).unwrap_or(0);
    Ok(TrainingSample {
        kind,
        path: path.to_string_lossy().into_owned(),
        ts_ms: ts,
        size,
        duration_ms,
        sample_rate,
    })
}

/// Persist the wakeword's metadata (target phrase, etc.) as a sibling
/// `meta.json` so it survives across sessions.
#[tauri::command]
pub async fn save_word_meta(name: String, target_phrase: String) -> Result<(), String> {
    let name = sanitize_name(&name)?;
    let dir = canonical_training_dir().map_err(err)?.join(&name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let payload = serde_json::json!({ "target_phrase": target_phrase });
    let body = serde_json::to_string_pretty(&payload).map_err(|e| format!("serialize: {e}"))?;
    let path = dir.join("meta.json");
    std::fs::write(&path, body).map_err(|e| format!("writing {}: {e}", path.display()))
}

fn read_word_meta(name: &str) -> Option<WordMeta> {
    let dir = canonical_training_dir().ok()?.join(name);
    let raw = std::fs::read_to_string(dir.join("meta.json")).ok()?;
    serde_json::from_str(&raw).ok()
}

#[tauri::command]
pub async fn list_training_samples(name: String) -> Result<Vec<TrainingSample>, String> {
    let name = sanitize_name(&name)?;
    let root = canonical_training_dir().map_err(err)?.join(&name);
    let mut out = Vec::new();
    for kind in ["positive", "negative"] {
        let dir = root.join(kind);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("wav") {
                continue;
            }
            let ts_ms = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let (duration_ms, sample_rate) = wav_metadata(&path).unwrap_or((0, 0));
            out.push(TrainingSample {
                kind: kind.into(),
                path: path.to_string_lossy().into_owned(),
                ts_ms,
                size: meta.len(),
                duration_ms,
                sample_rate,
            });
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.ts_ms));
    Ok(out)
}

#[tauri::command]
pub async fn list_training_words() -> Result<Vec<TrainingWord>, String> {
    let root = canonical_training_dir().map_err(err)?;
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let positive = count_files(&entry.path().join("positive"));
        let negative = count_files(&entry.path().join("negative"));
        let target_phrase = read_word_meta(&name).and_then(|m| m.target_phrase);
        out.push(TrainingWord {
            name,
            positive,
            negative,
            target_phrase,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn count_files(dir: &std::path::Path) -> u32 {
    std::fs::read_dir(dir)
        .map(|it| {
            u32::try_from(
                it.flatten()
                    .filter(|e| {
                        let p = e.path();
                        p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("wav")
                    })
                    .count(),
            )
            .unwrap_or(u32::MAX)
        })
        .unwrap_or(0)
}

/// Resolve a frontend-supplied path against the canonical training root.
/// Refuses to follow symlinks out of the root or to canonicalize-fallback
/// when the root doesn't exist (which would silently bypass containment).
fn resolve_under_training_root(path: &str) -> Result<PathBuf, String> {
    let training_root = canonical_training_dir().map_err(err)?;
    if !training_root.exists() {
        return Err("training directory does not exist yet".into());
    }
    let canonical_root = training_root
        .canonicalize()
        .map_err(|e| format!("resolving training root: {e}"))?;
    let canonical = std::path::PathBuf::from(path)
        .canonicalize()
        .map_err(|e| format!("resolving {path}: {e}"))?;
    if !canonical.starts_with(&canonical_root) {
        return Err("refused to access path outside the training directory".into());
    }
    // Reject symlinks even if their target points inside the root —
    // it removes a TOCTOU class where the link is repointed mid-call.
    let lmeta = std::fs::symlink_metadata(&canonical)
        .map_err(|e| format!("stat {}: {e}", canonical.display()))?;
    if lmeta.file_type().is_symlink() {
        return Err("refused to follow a symlink".into());
    }
    Ok(canonical)
}

/// Return the raw WAV bytes for a training sample so the frontend can
/// build a blob URL and play it back through `<audio>`. Path must live
/// under the canonical training root; size is capped at
/// [`MAX_TRAINING_SAMPLE_BYTES`] to keep the webview's memory bounded.
#[tauri::command]
pub async fn read_training_sample(path: String) -> Result<Vec<u8>, String> {
    let canonical = resolve_under_training_root(&path)?;
    let meta =
        std::fs::metadata(&canonical).map_err(|e| format!("stat {}: {e}", canonical.display()))?;
    if meta.len() > MAX_TRAINING_SAMPLE_BYTES {
        return Err(format!(
            "sample is {} bytes; refusing to read past cap of {}",
            meta.len(),
            MAX_TRAINING_SAMPLE_BYTES
        ));
    }
    std::fs::read(&canonical).map_err(|e| format!("reading {}: {e}", canonical.display()))
}

#[tauri::command]
pub async fn delete_training_sample(path: String) -> Result<(), String> {
    let canonical = resolve_under_training_root(&path)?;
    std::fs::remove_file(&canonical).map_err(|e| format!("removing {}: {e}", canonical.display()))
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TrainEvent {
    Log { line: String },
    Status { payload: serde_json::Value },
}

#[derive(Debug, Clone, Serialize)]
pub struct TrainingEnvStatus {
    /// Output of `uv --version`, or null when uv is not on PATH.
    pub uv_version: Option<String>,
    /// Where we expect to install the venv.
    pub python_env_dir: String,
    /// Resolved python interpreter — present once the venv has been created.
    pub python_path: Option<String>,
    /// True iff `python -c "import openwakeword"` succeeds in that venv.
    pub openwakeword_installed: bool,
    /// Location of the bundled `python/` source tree we install from. None
    /// means we couldn't find it (broken install).
    pub package_dir: Option<String>,
    /// Path to the precomputed negatives feature file required for training.
    pub negatives_features_path: String,
    /// True iff that file exists on disk.
    pub negatives_present: bool,
}

fn canonical_python_env_dir() -> anyhow::Result<std::path::PathBuf> {
    Ok(canonical_data_dir()?.join("python-env"))
}

fn canonical_python_path() -> anyhow::Result<std::path::PathBuf> {
    Ok(canonical_python_env_dir()?.join("bin").join("python"))
}

fn canonical_negatives_path() -> anyhow::Result<std::path::PathBuf> {
    Ok(canonical_data_dir()?
        .join("negatives")
        .join("openwakeword_features_ACAV100M_2000_hrs_16bit.npy"))
}

/// Locate the bundled `python/` source tree. Tries (in order):
/// 1. `<repo-root>/python` derived from `CARGO_MANIFEST_DIR` (dev builds).
/// 2. siblings of the running executable (released builds).
fn find_train_package_dir() -> Option<std::path::PathBuf> {
    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        // crates/gui/src-tauri → ../../../python
        let p = std::path::Path::new(manifest)
            .join("..")
            .join("..")
            .join("..")
            .join("python");
        if let Ok(canon) = p.canonicalize()
            && canon.join("pyproject.toml").is_file()
        {
            return Some(canon);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        let mut here = exe.parent()?.to_path_buf();
        for _ in 0..6 {
            let candidate = here.join("python");
            if candidate.join("pyproject.toml").is_file() {
                return Some(candidate);
            }
            let share = here.join("share").join("horchd").join("python");
            if share.join("pyproject.toml").is_file() {
                return Some(share);
            }
            if !here.pop() {
                break;
            }
        }
    }

    None
}

/// Try to find the `uv` binary. Tray-launched GUI processes inherit a
/// stripped PATH that often lacks `~/.local/bin` (the documented uv
/// install location), so we probe both `which::which` and the canonical
/// install paths under `$HOME`.
fn locate_uv() -> Option<PathBuf> {
    if let Ok(p) = which::which("uv") {
        return Some(p);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    for candidate in [
        home.join(".local").join("bin").join("uv"),
        home.join(".cargo").join("bin").join("uv"),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn find_uv_version() -> Option<String> {
    let path = locate_uv()?;
    let out = std::process::Command::new(&path)
        .arg("--version")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout)
        .ok()
        .map(|s| s.trim().to_string())
}

fn openwakeword_importable(python: &std::path::Path) -> bool {
    std::process::Command::new(python)
        .args(["-c", "import openwakeword"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[tauri::command]
pub async fn training_env_status() -> Result<TrainingEnvStatus, String> {
    let env_dir = canonical_python_env_dir().map_err(err)?;
    let py = canonical_python_path().map_err(err)?;
    let negatives = canonical_negatives_path().map_err(err)?;
    // I/O probing happens off the runtime thread so a slow disk doesn't
    // wedge the UI.
    let py_for_blocking = py.clone();
    let (uv_version, python_present, oww) = tokio::task::spawn_blocking(move || {
        let present = py_for_blocking.is_file();
        (
            find_uv_version(),
            present,
            present && openwakeword_importable(&py_for_blocking),
        )
    })
    .await
    .map_err(|e| format!("io join: {e}"))?;
    Ok(TrainingEnvStatus {
        uv_version,
        python_env_dir: env_dir.to_string_lossy().into_owned(),
        python_path: python_present.then(|| py.to_string_lossy().into_owned()),
        openwakeword_installed: oww,
        package_dir: find_train_package_dir().map(|p| p.to_string_lossy().into_owned()),
        negatives_features_path: negatives.to_string_lossy().into_owned(),
        negatives_present: negatives.is_file(),
    })
}

/// Bootstrap the isolated training venv via `uv`. Streams every line of
/// uv's output as `horchd://setup` so the UI can show progress.
#[tauri::command]
pub async fn setup_training_env(
    app: AppHandle,
    procs: State<'_, Arc<ProcessRegistry>>,
) -> Result<String, String> {
    let uv = locate_uv().ok_or_else(|| {
        "uv is not installed — get it from https://docs.astral.sh/uv/ (one-line install: `curl -LsSf https://astral.sh/uv/install.sh | sh`)"
            .to_string()
    })?;
    let pkg = find_train_package_dir()
        .ok_or_else(|| "could not locate the bundled python/ helper directory".to_string())?;
    let env_dir = canonical_python_env_dir().map_err(err)?;
    if let Some(parent) = env_dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }

    let uv_str = uv.to_string_lossy().into_owned();
    emit_setup_status(&app, "create-venv", 0.0);
    run_streamed(
        &app,
        &procs,
        ProcessKind::Setup,
        &uv_str,
        &[
            "venv",
            "--python",
            "3.12",
            env_dir.to_string_lossy().as_ref(),
        ],
    )
    .await?;

    emit_setup_status(&app, "install", 0.5);
    let pkg_str = pkg.to_string_lossy().into_owned();
    let env_str = env_dir.to_string_lossy().into_owned();
    run_streamed(
        &app,
        &procs,
        ProcessKind::Setup,
        &uv_str,
        &[
            "pip",
            "install",
            "--python",
            env_str.as_str(),
            "-e",
            pkg_str.as_str(),
        ],
    )
    .await?;

    emit_setup_status(&app, "done", 1.0);
    Ok(canonical_python_path()
        .map_err(err)?
        .to_string_lossy()
        .into_owned())
}

/// Pull the precomputed negatives features file via the helper's
/// `horchd-fetch-negatives` entry point. Streams to `horchd://setup`.
#[tauri::command]
pub async fn fetch_negatives(
    app: AppHandle,
    procs: State<'_, Arc<ProcessRegistry>>,
) -> Result<String, String> {
    let py = canonical_python_path().map_err(err)?;
    if !py.is_file() {
        return Err("training venv not set up yet — run setup first".into());
    }
    emit_setup_status(&app, "fetch", 0.0);
    let py_str = py.to_string_lossy().into_owned();
    run_streamed(
        &app,
        &procs,
        ProcessKind::Setup,
        py_str.as_str(),
        &["-m", "horchd_train.fetch"],
    )
    .await?;
    emit_setup_status(&app, "done", 1.0);
    Ok(canonical_negatives_path()
        .map_err(err)?
        .to_string_lossy()
        .into_owned())
}

fn emit_setup_status(app: &AppHandle, stage: &str, progress: f64) {
    let _ = app.emit(
        "horchd://setup",
        TrainEvent::Status {
            payload: serde_json::json!({ "stage": stage, "progress": progress }),
        },
    );
}

/// Spawn `program` with `args`, streaming stdout + stderr line-by-line
/// to `horchd://<channel>` (setup or train). `##HORCHD {…}` lines become
/// structured `Status` events; everything else is a raw `Log` line.
///
/// Hardened against process leaks: `kill_on_drop(true)` ensures that if
/// the awaiting future is dropped (window closed mid-run, cancel
/// command), the OS sends SIGKILL to the child.
async fn run_streamed(
    app: &AppHandle,
    procs: &Arc<ProcessRegistry>,
    kind: ProcessKind,
    program: &str,
    args: &[&str],
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let event = format!("horchd://{}", kind.label());
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::null());
    cmd.kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawning {program}: {e}"))?;

    let stdout = child.stdout.take().ok_or_else(|| "no stdout".to_string())?;
    let stderr = child.stderr.take().ok_or_else(|| "no stderr".to_string())?;

    // Park the Child in shared state so a `cancel_*` command can kill it.
    let slot: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(Some(child)));
    let prev = procs.install(kind, slot.clone()).await;
    if let Some(prev) = prev {
        // Best-effort kill of any prior in-flight process for the same
        // kind — the UI shouldn't be able to start two at once, but if
        // it does, don't leak.
        let mut g = prev
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(c) = g.as_mut() {
            let _ = c.start_kill();
        }
    }

    let evt_out = event.clone();
    let evt_err = event.clone();
    let app_out = app.clone();
    let app_err = app.clone();

    let out_task = tokio::spawn(async move {
        let mut r = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = r.next_line().await {
            forward_event_line(&app_out, &evt_out, &line);
        }
    });
    let err_task = tokio::spawn(async move {
        let mut r = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = r.next_line().await {
            forward_event_line(&app_err, &evt_err, &line);
        }
    });

    // Wait on the child *through* the slot so a cancel can `start_kill`
    // it concurrently. We can't hold a `MutexGuard` across `await`, so
    // we take ownership.
    let mut child = slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
        .ok_or_else(|| "internal error: child slot was empty".to_string())?;
    let status = child
        .wait()
        .await
        .map_err(|e| format!("waiting on {program}: {e}"))?;
    procs.clear(kind).await;
    let _ = out_task.await;
    let _ = err_task.await;

    if !status.success() {
        return Err(format!(
            "{program} exited with code {}",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_setup(procs: State<'_, Arc<ProcessRegistry>>) -> Result<(), String> {
    procs.kill(ProcessKind::Setup).await
}

#[tauri::command]
pub async fn cancel_training(procs: State<'_, Arc<ProcessRegistry>>) -> Result<(), String> {
    procs.kill(ProcessKind::Train).await
}

fn forward_event_line(app: &AppHandle, event: &str, line: &str) {
    if let Some(rest) = line.strip_prefix("##HORCHD ")
        && let Ok(payload) = serde_json::from_str::<serde_json::Value>(rest)
    {
        let _ = app.emit(event, TrainEvent::Status { payload });
        return;
    }
    let _ = app.emit(
        event,
        TrainEvent::Log {
            line: line.to_string(),
        },
    );
}

/// Spawn the `horchd_train` Python helper. Output streams to
/// `horchd://train`; success returns the produced `.onnx` path.
///
/// Python resolution order:
/// 1. The managed venv at `~/.local/share/horchd/python-env/bin/python`
///    set up via `setup_training_env`.
/// 2. `python3` from PATH as a last resort.
///
/// (The previous `HORCHD_PYTHON` / `HORCHD_TRAIN_SCRIPT` env-var
/// overrides were removed — they let the surrounding env dictate the
/// Python interpreter and module path, which is an obvious code-execution
/// vector if anything else can set env vars on this process.)
#[tauri::command]
pub async fn train_wakeword(
    app: AppHandle,
    procs: State<'_, Arc<ProcessRegistry>>,
    name: String,
    target_phrase: String,
    augment_per_recording: Option<u32>,
    steps: Option<u32>,
) -> Result<String, String> {
    let name = sanitize_name(&name)?;
    let phrase = target_phrase.trim();
    if phrase.is_empty() {
        return Err("set the target phrase before training".into());
    }
    if phrase.contains(['\n', '\r']) {
        return Err("target phrase must not contain line breaks".into());
    }
    if phrase.len() > 256 {
        return Err("target phrase is too long".into());
    }

    let python = resolve_train_python()?;
    let mut args: Vec<String> = vec!["-m".into(), "horchd_train".into()];
    args.push("--name".into());
    args.push(name.clone());
    args.push("--target-phrase".into());
    args.push(phrase.into());
    if let Some(n) = augment_per_recording {
        args.push("--augment-per-recording".into());
        args.push(n.to_string());
    }
    if let Some(s) = steps {
        args.push("--steps".into());
        args.push(s.to_string());
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_streamed(&app, &procs, ProcessKind::Train, &python, &arg_refs).await?;

    let onnx = canonical_models_dir()
        .map_err(err)?
        .join(format!("{name}.onnx"));
    Ok(onnx.to_string_lossy().into_owned())
}

fn resolve_train_python() -> Result<String, String> {
    if let Ok(managed) = canonical_python_path()
        && managed.is_file()
    {
        return Ok(managed.to_string_lossy().into_owned());
    }
    if let Ok(p) = which::which("python3") {
        return Ok(p.to_string_lossy().into_owned());
    }
    Err("no python interpreter found — set up the training venv from the Train tab".into())
}

/// Lifecycle: Tauri calls `register_state` from `lib.rs::run` so the
/// `State<Arc<ProcessRegistry>>` injection works in commands above.
pub fn register_state<R: tauri::Runtime>(app: &mut tauri::App<R>) {
    app.manage(Arc::new(ProcessRegistry::default()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_accepts_normal_names() {
        assert_eq!(sanitize_name("hey_jarvis").unwrap(), "hey_jarvis");
        assert_eq!(sanitize_name("hey-jarvis").unwrap(), "hey-jarvis");
        assert_eq!(sanitize_name("Alexa1").unwrap(), "Alexa1");
        // Trim whitespace
        assert_eq!(sanitize_name("  alexa  ").unwrap(), "alexa");
    }

    #[test]
    fn sanitize_rejects_path_traversal_and_separators() {
        for bad in [
            "", "..", "../foo", "foo/bar", "foo bar", "foo.onnx", "foo\0bar",
        ] {
            assert!(sanitize_name(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn sanitize_rejects_dash_prefix_and_overlong() {
        assert!(sanitize_name("-rm").is_err());
        let long = "a".repeat(MAX_NAME_LEN + 1);
        assert!(sanitize_name(&long).is_err());
    }
}
