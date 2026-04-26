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
pub async fn reload() -> Result<(), String> {
    let p = proxy().await.map_err(err)?;
    p.reload().await.map_err(|e| err(e.into()))
}

#[tauri::command]
pub async fn models_dir() -> Result<String, String> {
    Ok(canonical_models_dir().map_err(err)?.to_string_lossy().into_owned())
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
            std::fs::copy(&sidecar, &sidecar_dest)
                .map_err(|e| format!("copying sidecar: {e}"))?;
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
    use std::path::PathBuf;
    let base = std::env::var_os("XDG_DATA_HOME").map_or_else(
        || -> anyhow::Result<PathBuf> {
            let home = std::env::var_os("HOME")
                .ok_or_else(|| anyhow::anyhow!("$HOME is not set"))?;
            Ok(PathBuf::from(home).join(".local").join("share"))
        },
        |v| Ok(PathBuf::from(v)),
    )?;
    Ok(base.join("horchd").join("models"))
}
