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

/// Spawn the `horchd_train` Python helper as a subprocess. Each line of
/// stdout/stderr is forwarded to the frontend via `horchd://train` so
/// the UI can render a live log + progress bar.
///
/// Resolves the python executable in this order:
/// 1. `$HORCHD_PYTHON` if set (use this for a dedicated venv).
/// 2. `python3` from `$PATH`.
///
/// Resolves the helper in this order:
/// 1. `$HORCHD_TRAIN_SCRIPT` if set — runs `python <path>` directly.
/// 2. The package itself: assumes the chosen python can `python -m
///    horchd_train` (i.e. the user installed it via `uv sync` or `pip
///    install -e python/`).
#[tauri::command]
pub async fn train_wakeword(
    app: AppHandle,
    name: String,
    target_phrase: String,
    augment_per_recording: Option<u32>,
    steps: Option<u32>,
) -> Result<String, String> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command;

    let name = sanitize_name(&name)?;
    let phrase = target_phrase.trim();
    if phrase.is_empty() {
        return Err("set the target phrase before training".into());
    }

    let python = std::env::var("HORCHD_PYTHON").unwrap_or_else(|_| "python3".into());
    let mut cmd = Command::new(&python);
    if let Ok(script) = std::env::var("HORCHD_TRAIN_SCRIPT") {
        cmd.arg(script);
    } else {
        cmd.args(["-m", "horchd_train"]);
    }
    cmd.arg("--name")
        .arg(&name)
        .arg("--target-phrase")
        .arg(phrase);
    if let Some(n) = augment_per_recording {
        cmd.arg("--augment-per-recording").arg(n.to_string());
    }
    if let Some(s) = steps {
        cmd.arg("--steps").arg(s.to_string());
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::null());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawning {python}: {e} — install the helper with `uv sync --project python/` and ensure $HORCHD_PYTHON points at the venv"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "no stdout from training subprocess".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "no stderr from training subprocess".to_string())?;

    let app_for_stdout = app.clone();
    let app_for_stderr = app.clone();

    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            forward_train_line(&app_for_stdout, &line);
        }
    });
    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            forward_train_line(&app_for_stderr, &line);
        }
    });

    let status = child
        .wait()
        .await
        .map_err(|e| format!("waiting on training subprocess: {e}"))?;
    let _ = stdout_task.await;
    let _ = stderr_task.await;

    if !status.success() {
        return Err(format!(
            "training subprocess exited with code {}",
            status.code().unwrap_or(-1)
        ));
    }

    let onnx = canonical_models_dir()
        .map_err(err)?
        .join(format!("{name}.onnx"));
    Ok(onnx.to_string_lossy().into_owned())
}

fn forward_train_line(app: &AppHandle, line: &str) {
    if let Some(rest) = line.strip_prefix("##HORCHD ")
        && let Ok(payload) = serde_json::from_str::<serde_json::Value>(rest)
    {
        let _ = app.emit("horchd://train", TrainEvent::Status { payload });
        return;
    }
    let _ = app.emit(
        "horchd://train",
        TrainEvent::Log {
            line: line.to_string(),
        },
    );
}
