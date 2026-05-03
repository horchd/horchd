//! Per-connection Wyoming state machine.
//!
//! Mode 1 (server-owned mic, the only mode wired today): subscribe to
//! the live mic pipeline's Detection broadcast, fan out as Wyoming
//! `detection` events to this client. Client-side `audio-*` events are
//! tolerated but ignored — the server already owns the audio source.
//!
//! Mode 2 / Hybrid land in D3, behind the same `tokio::select!` skeleton
//! plus a per-connection `WyomingSource` and `TransientPipeline`.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use horchd_wyoming::event::{Eventable, read_event, write_event};
use horchd_wyoming::wake::{Detect, Detection as WyoDetection};
use tokio::io::{AsyncBufRead, AsyncWrite};
use tokio::sync::broadcast::error::RecvError;

use crate::wyoming::info::build_info;
use crate::wyoming::listener::ServerCtx;

/// Drive a single Wyoming connection until either the peer disconnects
/// or the broadcast feeding our detections closes.
///
/// `peer` is a free-form identifier for logs (`"127.0.0.1:54321"`,
/// `"unix-peer-3"`, `"stdio"`). We don't use it for routing.
pub async fn handle<R, W>(
    mut reader: R,
    mut writer: W,
    ctx: Arc<ServerCtx>,
    peer: String,
) -> Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    tracing::info!(%peer, "Wyoming client connected");

    let mut detections = ctx.pipeline.subscribe_detections();
    let mut filter: Option<Vec<String>> = None;
    let session_started = Instant::now();

    loop {
        tokio::select! {
            biased;
            evt = read_event(&mut reader) => match evt? {
                None => break,
                Some(evt) => match evt.event_type.as_str() {
                    "describe" => {
                        let info = {
                            let s = ctx.state.lock().await;
                            build_info(&s)
                        };
                        write_event(&mut writer, &info.into_event()).await?;
                    }
                    "detect" => {
                        let det = Detect::from_event(&evt)?;
                        filter = (!det.names.is_empty()).then_some(det.names);
                        tracing::debug!(%peer, ?filter, "wakeword filter set");
                    }
                    // Mode 1 ignores client-pushed audio: the server is
                    // already streaming from its own mic. The events are
                    // valid Wyoming, just irrelevant in this topology.
                    "audio-start" | "audio-chunk" | "audio-stop" => {
                        tracing::debug!(%peer, ty = %evt.event_type, "ignored in local-mic mode");
                    }
                    other => {
                        tracing::debug!(%peer, ty = %other, "unhandled event type");
                    }
                },
            },
            recv = detections.recv() => match recv {
                Ok(det) => {
                    if let Some(names) = filter.as_ref()
                        && !names.iter().any(|n| n == &det.name)
                    {
                        continue;
                    }
                    let timestamp_ms = u64::try_from(session_started.elapsed().as_millis())
                        .unwrap_or(u64::MAX);
                    let wyo = WyoDetection {
                        name: det.name.clone(),
                        timestamp: Some(timestamp_ms),
                        speaker: None,
                    };
                    write_event(&mut writer, &wyo.into_event()).await?;
                    tracing::debug!(%peer, name = %det.name, score = det.score, "forwarded detection");
                },
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!(%peer, skipped = n, "Wyoming detection broadcast lagged");
                }
                Err(RecvError::Closed) => {
                    tracing::info!(%peer, "detection broadcast closed; ending session");
                    break;
                }
            },
        }
    }

    tracing::info!(%peer, "Wyoming client disconnected");
    Ok(())
}
