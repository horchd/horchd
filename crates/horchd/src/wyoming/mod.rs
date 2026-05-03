//! Wyoming-protocol server embedded in the daemon.
//!
//! Lives inside `horchd` (not as its own crate) because the server needs
//! direct access to the live `Pipeline` and `SharedState` — splitting it
//! out would force re-exporting half of the daemon's internals through a
//! public surface no external consumer needs. The wire-level codec lives
//! in the standalone `horchd-wyoming` crate; this module is the listener,
//! the per-connection state machine, and the `Info` builder.

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::task::JoinHandle;

use crate::pipeline::Pipeline;
use crate::state::SharedState;

pub mod handler;
pub mod info;
pub mod listener;
pub mod uri;
pub mod zeroconf;

pub use listener::{ServerCtx, serve};
pub use uri::{ListenAddr, parse as parse_uri};
pub use zeroconf::{ZeroconfHandle, announce as announce_zeroconf};

/// Lifetime handles for one running Wyoming server instance — bound
/// listeners + (optional) mDNS announcement. Dropping this stops the
/// accept loops and unregisters the mDNS service. In-flight client
/// connections survive the drop and run until the peer closes.
pub struct WyomingHandles {
    listeners: Vec<JoinHandle<()>>,
    _zeroconf: Option<ZeroconfHandle>,
}

impl WyomingHandles {
    pub fn stop(self) {
        for handle in self.listeners {
            handle.abort();
        }
        // _zeroconf drops here, which unregisters from mDNS.
    }
}

/// Bind every configured Wyoming listener and (if requested) announce
/// over mDNS. Reads the current `[wyoming]` block from `state` so it's
/// safe to call after the user edited the config.
///
/// Returns `Ok(None)` when `enabled = false` — the server is simply off.
pub async fn start(
    state: &SharedState,
    pipeline: &Arc<Pipeline>,
) -> Result<Option<WyomingHandles>> {
    let (enabled, mode, listen, zeroconf, service_name) = {
        let s = state.lock().await;
        (
            s.config.wyoming.enabled,
            s.config.wyoming.mode,
            s.config.wyoming.listen.clone(),
            s.config.wyoming.zeroconf,
            s.config.wyoming.service_name.clone(),
        )
    };
    if !enabled {
        return Ok(None);
    }

    let addrs = listen
        .iter()
        .map(|u| parse_uri(u))
        .collect::<Result<Vec<_>>>()
        .context("parsing [wyoming].listen URIs")?;

    let ctx = Arc::new(ServerCtx {
        state: Arc::clone(state),
        pipeline: Arc::clone(pipeline),
        mode,
    });
    let listeners = serve(addrs.clone(), ctx).await?;

    let zeroconf_handle = if zeroconf {
        let name = service_name.unwrap_or_else(default_service_name);
        match announce_zeroconf(&addrs, &name) {
            Ok(h) => h,
            Err(err) => {
                tracing::warn!(
                    ?err,
                    "mDNS announcement failed; Wyoming still reachable by IP"
                );
                None
            }
        }
    } else {
        None
    };

    Ok(Some(WyomingHandles {
        listeners,
        _zeroconf: zeroconf_handle,
    }))
}

fn default_service_name() -> String {
    let host = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "horchd".into());
    let suffix: String = host
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    if suffix.is_empty() {
        "horchd".into()
    } else {
        format!("horchd-{suffix}")
    }
}
