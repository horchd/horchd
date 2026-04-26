//! `#[tauri::command]` handlers. The frontend calls these via
//! `invoke()`; each one delegates to the D-Bus proxy.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::dbus_client::proxy;

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
    let dest_dir = canonical_models_dir().map_err(err)?;
    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| format!("creating {}: {}", dest_dir.display(), e))?;
    let dest = dest_dir.join(format!("{name}.onnx"));

    let src = std::path::PathBuf::from(&source_path);
    if !src.exists() {
        return Err(format!("source file not found: {source_path}"));
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
    use std::path::PathBuf;
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
        .map(|d| d.as_millis() as u64)
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
    let frames = reader.duration();
    let duration_ms = ((frames as u64) * 1000 / spec.sample_rate as u64) as u32;
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
/// `samples` is the raw int16 stream the AudioWorklet captured, so no
/// transcoding is needed — we control the format end-to-end.
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
    std::fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let ts = now_ms();
    let path = dir.join(format!("{ts}.wav"));
    write_pcm_wav(&path, &samples, sample_rate)?;

    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let duration_ms = ((samples.len() as u64) * 1000 / u64::from(sample_rate)) as u32;
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
    let path = dir.join("meta.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&payload).unwrap_or_default(),
    )
    .map_err(|e| format!("writing {}: {e}", path.display()))
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
            it.flatten()
                .filter(|e| {
                    let p = e.path();
                    p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("wav")
                })
                .count() as u32
        })
        .unwrap_or(0)
}

/// Return the raw WAV bytes for a training sample so the frontend can
/// build a blob URL and play it back through `<audio>`. Path must live
/// under the canonical training root.
#[tauri::command]
pub async fn read_training_sample(path: String) -> Result<Vec<u8>, String> {
    let p = std::path::PathBuf::from(&path);
    let training_root = canonical_training_dir().map_err(err)?;
    let canonical_root = training_root
        .canonicalize()
        .unwrap_or_else(|_| training_root.clone());
    let canonical = p
        .canonicalize()
        .map_err(|e| format!("resolving {path}: {e}"))?;
    if !canonical.starts_with(&canonical_root) {
        return Err("refused to read outside the training directory".into());
    }
    std::fs::read(&canonical).map_err(|e| format!("reading {}: {e}", canonical.display()))
}

#[tauri::command]
pub async fn delete_training_sample(path: String) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);
    let training_root = canonical_training_dir().map_err(err)?;
    let canonical_root = training_root
        .canonicalize()
        .unwrap_or_else(|_| training_root.clone());
    let canonical = p
        .canonicalize()
        .map_err(|e| format!("resolving {path}: {e}"))?;
    if !canonical.starts_with(&canonical_root) {
        return Err("refused to delete outside the training directory".into());
    }
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
/// 1. `$HORCHD_TRAIN_PACKAGE_DIR`
/// 2. `<repo-root>/python` derived from CARGO_MANIFEST_DIR (dev builds)
/// 3. siblings of the running executable (released builds)
fn find_train_package_dir() -> Option<std::path::PathBuf> {
    if let Ok(env_dir) = std::env::var("HORCHD_TRAIN_PACKAGE_DIR") {
        let p = std::path::PathBuf::from(env_dir);
        if p.join("pyproject.toml").is_file() {
            return Some(p);
        }
    }

    if let Some(manifest) = option_env!("CARGO_MANIFEST_DIR") {
        // crates/horchd-gui/src-tauri → ../../../python
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

fn find_uv() -> Option<String> {
    use std::process::Command;
    let candidates = ["uv"];
    for name in candidates {
        if let Ok(out) = Command::new(name).arg("--version").output()
            && out.status.success()
        {
            return String::from_utf8(out.stdout)
                .ok()
                .map(|s| s.trim().to_string());
        }
    }
    None
}

fn openwakeword_importable(python: &std::path::Path) -> bool {
    use std::process::Command;
    Command::new(python)
        .args(["-c", "import openwakeword"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[tauri::command]
pub async fn training_env_status() -> Result<TrainingEnvStatus, String> {
    let env_dir = canonical_python_env_dir().map_err(err)?;
    let py = canonical_python_path().map_err(err)?;
    let negatives = canonical_negatives_path().map_err(err)?;
    let python_present = py.is_file();
    Ok(TrainingEnvStatus {
        uv_version: find_uv(),
        python_env_dir: env_dir.to_string_lossy().into_owned(),
        python_path: python_present.then(|| py.to_string_lossy().into_owned()),
        openwakeword_installed: python_present && openwakeword_importable(&py),
        package_dir: find_train_package_dir().map(|p| p.to_string_lossy().into_owned()),
        negatives_features_path: negatives.to_string_lossy().into_owned(),
        negatives_present: negatives.is_file(),
    })
}

/// Bootstrap the isolated training venv via `uv`. Streams every line of
/// uv's output as `horchd://setup` so the UI can show progress.
#[tauri::command]
pub async fn setup_training_env(app: AppHandle) -> Result<String, String> {
    if find_uv().is_none() {
        return Err(
            "uv is not installed — get it from https://docs.astral.sh/uv/ (one-line install: `curl -LsSf https://astral.sh/uv/install.sh | sh`)"
                .into(),
        );
    }
    let pkg = find_train_package_dir()
        .ok_or_else(|| "could not locate the bundled python/ helper directory".to_string())?;
    let env_dir = canonical_python_env_dir().map_err(err)?;
    if let Some(parent) = env_dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }

    emit_setup_status(&app, "create-venv", 0.0);
    run_streamed(
        &app,
        "uv",
        &[
            "venv",
            "--python",
            "3.12",
            env_dir.to_string_lossy().as_ref(),
        ],
        None,
    )
    .await?;

    emit_setup_status(&app, "install", 0.5);
    let pkg_str = pkg.to_string_lossy().into_owned();
    let env_str = env_dir.to_string_lossy().into_owned();
    run_streamed(
        &app,
        "uv",
        &[
            "pip",
            "install",
            "--python",
            env_str.as_str(),
            "-e",
            pkg_str.as_str(),
        ],
        None,
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
pub async fn fetch_negatives(app: AppHandle) -> Result<String, String> {
    let py = canonical_python_path().map_err(err)?;
    if !py.is_file() {
        return Err("training venv not set up yet — run setup first".into());
    }
    emit_setup_status(&app, "fetch", 0.0);
    let py_str = py.to_string_lossy().into_owned();
    run_streamed(
        &app,
        py_str.as_str(),
        &["-m", "horchd_train.fetch"],
        Some("setup"),
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
/// to `horchd://<channel>` (or `horchd://setup` by default). `##HORCHD
/// {…}` lines become structured `Status` events; everything else is a
/// raw `Log` line.
async fn run_streamed(
    app: &AppHandle,
    program: &str,
    args: &[&str],
    channel: Option<&str>,
) -> Result<(), String> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command;

    let event = format!("horchd://{}", channel.unwrap_or("setup"));
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::null());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawning {program}: {e}"))?;

    let stdout = child.stdout.take().ok_or_else(|| "no stdout".to_string())?;
    let stderr = child.stderr.take().ok_or_else(|| "no stderr".to_string())?;

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

    let status = child
        .wait()
        .await
        .map_err(|e| format!("waiting on {program}: {e}"))?;
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
/// 1. `$HORCHD_PYTHON` (lets advanced users point at a custom venv).
/// 2. The managed venv at `~/.local/share/horchd/python-env/bin/python`
///    set up via `setup_training_env` from the GUI.
/// 3. `python3` from `$PATH` as a last resort.
#[tauri::command]
pub async fn train_wakeword(
    app: AppHandle,
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

    let python = resolve_train_python()?;
    let mut args: Vec<String> = Vec::new();
    if let Ok(script) = std::env::var("HORCHD_TRAIN_SCRIPT") {
        args.push(script);
    } else {
        args.push("-m".into());
        args.push("horchd_train".into());
    }
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
    run_streamed(&app, &python, &arg_refs, Some("train")).await?;

    let onnx = canonical_models_dir()
        .map_err(err)?
        .join(format!("{name}.onnx"));
    Ok(onnx.to_string_lossy().into_owned())
}

fn resolve_train_python() -> Result<String, String> {
    if let Ok(env) = std::env::var("HORCHD_PYTHON") {
        return Ok(env);
    }
    if let Ok(managed) = canonical_python_path()
        && managed.is_file()
    {
        return Ok(managed.to_string_lossy().into_owned());
    }
    Ok("python3".into())
}
