//! Pumps `xyz.horchd.Daemon1` D-Bus signals into Tauri frontend events:
//!
//! - `Detected`      → `horchd://detected`
//! - `ScoreSnapshot` → `horchd://score`
//!
//! Reconnects on broken streams so a daemon restart doesn't silently
//! kill the UI tickers.

use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

use crate::dbus_client;

const DETECTED_EVENT: &str = "horchd://detected";
const SCORE_EVENT: &str = "horchd://score";
/// Initial reconnect delay; we double up to MAX_RECONNECT_DELAY on each
/// consecutive failure so a permanently-down daemon doesn't hammer the
/// session bus at 0.5 Hz forever.
const INITIAL_RECONNECT_DELAY: Duration = Duration::from_millis(500);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize)]
struct DetectedPayload<'a> {
    name: &'a str,
    score: f64,
    timestamp_us: u64,
    /// Wall-clock millis at receipt time, so the UI can compute
    /// "x seconds ago" without re-reading the daemon's monotonic clock.
    received_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct ScorePayload<'a> {
    name: &'a str,
    score: f64,
}

pub fn spawn<R: Runtime>(handle: AppHandle<R>) {
    let h1 = handle.clone();
    tauri::async_runtime::spawn(async move {
        let mut delay = INITIAL_RECONNECT_DELAY;
        loop {
            match run_detected(&h1).await {
                Ok(()) => {
                    tracing::warn!("Detected stream ended cleanly; reconnecting");
                    delay = INITIAL_RECONNECT_DELAY;
                }
                Err(err) => {
                    tracing::warn!(?err, "Detected stream errored; reconnecting");
                    delay = (delay * 2).min(MAX_RECONNECT_DELAY);
                }
            }
            tokio::time::sleep(delay).await;
        }
    });
    tauri::async_runtime::spawn(async move {
        let mut delay = INITIAL_RECONNECT_DELAY;
        loop {
            match run_scores(&handle).await {
                Ok(()) => {
                    tracing::warn!("ScoreSnapshot stream ended cleanly; reconnecting");
                    delay = INITIAL_RECONNECT_DELAY;
                }
                Err(err) => {
                    tracing::warn!(?err, "ScoreSnapshot stream errored; reconnecting");
                    delay = (delay * 2).min(MAX_RECONNECT_DELAY);
                }
            }
            tokio::time::sleep(delay).await;
        }
    });
}

async fn run_detected<R: Runtime>(handle: &AppHandle<R>) -> Result<()> {
    let proxy = dbus_client::proxy()
        .await
        .context("opening proxy for Detected subscription")?;
    let mut stream = proxy
        .receive_detected()
        .await
        .context("subscribing to Detected signal")?;
    tracing::info!("subscribed to {DETECTED_EVENT}");

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
        if let Err(err) = handle.emit(DETECTED_EVENT, payload) {
            tracing::warn!(?err, "emitting frontend Detected event");
        }
    }
    Ok(())
}

async fn run_scores<R: Runtime>(handle: &AppHandle<R>) -> Result<()> {
    let proxy = dbus_client::proxy()
        .await
        .context("opening proxy for ScoreSnapshot subscription")?;
    let mut stream = proxy
        .receive_score_snapshot()
        .await
        .context("subscribing to ScoreSnapshot signal")?;
    tracing::info!("subscribed to {SCORE_EVENT}");

    while let Some(sig) = stream.next().await {
        let args = match sig.args() {
            Ok(a) => a,
            Err(err) => {
                tracing::warn!(?err, "malformed ScoreSnapshot payload");
                continue;
            }
        };
        let payload = ScorePayload {
            name: args.name,
            score: args.score,
        };
        if let Err(err) = handle.emit(SCORE_EVENT, payload) {
            tracing::warn!(?err, "emitting frontend ScoreSnapshot event");
        }
    }
    Ok(())
}

fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
