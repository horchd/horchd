//! `#[tauri::command]` handlers. The frontend calls these via
//! `invoke()`; each one delegates to the D-Bus proxy.

use serde::Serialize;

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
}

#[derive(Debug, Serialize)]
pub struct TrainingWord {
    pub name: String,
    pub positive: u32,
    pub negative: u32,
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

fn ext_for(mime: &str) -> &'static str {
    if mime.contains("webm") {
        "webm"
    } else if mime.contains("ogg") {
        "ogg"
    } else if mime.contains("wav") {
        "wav"
    } else {
        "bin"
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[tauri::command]
pub async fn training_dir() -> Result<String, String> {
    Ok(canonical_training_dir()
        .map_err(err)?
        .to_string_lossy()
        .into_owned())
}

#[tauri::command]
pub async fn save_training_sample(
    name: String,
    kind: String,
    mime: String,
    data: Vec<u8>,
) -> Result<TrainingSample, String> {
    let name = sanitize_name(&name)?;
    let kind = match kind.as_str() {
        "positive" | "negative" => kind,
        _ => return Err(format!("unknown sample kind: {kind}")),
    };
    let dir = canonical_training_dir()
        .map_err(err)?
        .join(&name)
        .join(&kind);
    std::fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let ts = now_ms();
    let path = dir.join(format!("{ts}.{}", ext_for(&mime)));
    let size = data.len() as u64;
    std::fs::write(&path, data).map_err(|e| format!("writing {}: {e}", path.display()))?;
    Ok(TrainingSample {
        kind,
        path: path.to_string_lossy().into_owned(),
        ts_ms: ts,
        size,
    })
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
            let ts_ms = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            out.push(TrainingSample {
                kind: kind.into(),
                path: path.to_string_lossy().into_owned(),
                ts_ms,
                size: meta.len(),
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
        out.push(TrainingWord {
            name,
            positive,
            negative,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn count_files(dir: &std::path::Path) -> u32 {
    std::fs::read_dir(dir)
        .map(|it| it.flatten().filter(|e| e.path().is_file()).count() as u32)
        .unwrap_or(0)
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

/// Placeholder for the future in-app training pipeline. Until the ML
/// backend ships, this errors out with a pointer to the alternative.
#[tauri::command]
pub async fn train_wakeword(_name: String) -> Result<String, String> {
    Err(
        "in-app training is not implemented yet — for now, train via openWakeWord and import the .onnx"
            .into(),
    )
}
