//! Pumps `xyz.horchd.Daemon1.Detected` D-Bus signals into Tauri
//! frontend events on `horchd://detected`. Reconnects on broken
//! streams so a daemon restart doesn't silently kill the UI ticker.

use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

use crate::dbus_client;

const EVENT_NAME: &str = "horchd://detected";
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize)]
struct DetectedPayload<'a> {
    name: &'a str,
    score: f64,
    timestamp_us: u64,
    /// Wall-clock millis at receipt time, so the UI can compute
    /// "x seconds ago" without re-reading the daemon's monotonic clock.
    received_unix_ms: u64,
}

pub fn spawn<R: Runtime>(handle: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        loop {
            match run_once(&handle).await {
                Ok(()) => tracing::warn!("Detected stream ended cleanly; reconnecting"),
                Err(err) => tracing::warn!(?err, "Detected stream errored; reconnecting"),
            }
            tokio::time::sleep(RECONNECT_DELAY).await;
        }
    });
}

async fn run_once<R: Runtime>(handle: &AppHandle<R>) -> Result<()> {
    let proxy = dbus_client::proxy()
        .await
        .context("opening proxy for Detected subscription")?;
    let mut stream = proxy
        .receive_detected()
        .await
        .context("subscribing to Detected signal")?;
    tracing::info!("subscribed to {EVENT_NAME}");

    while let Some(sig) = stream.next().await {
        let args = match sig.args() {
            Ok(a) => a,
            Err(err) => {
                tracing::warn!(?err, "malformed Detected payload");
                continue;
            }
        };
        let payload = DetectedPayload {
            name: args.name,
            score: args.score,
            timestamp_us: args.timestamp_us,
            received_unix_ms: now_unix_ms(),
        };
        if let Err(err) = handle.emit(EVENT_NAME, payload) {
            tracing::warn!(?err, "emitting frontend event");
        }
    }
    Ok(())
}

fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
