//! Multi-transport Wyoming listener.
//!
//! `serve` spawns one accept loop per [`ListenAddr`] and one
//! per-connection task per accepted client. All tasks live as long as
//! the tokio runtime — graceful shutdown is the runtime's responsibility
//! at process exit. A future revision can thread a
//! `tokio_util::sync::CancellationToken` through if we ever need
//! mid-flight teardown (e.g. `WyomingStop` D-Bus call).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use horchd_client::WyomingMode;
use tokio::io::BufReader;
use tokio::net::{TcpListener, UnixListener};
use tokio::task::JoinHandle;

use crate::pipeline::Pipeline;
use crate::state::SharedState;
use crate::wyoming::handler::handle;
use crate::wyoming::uri::ListenAddr;

/// Snapshot of the daemon-side handles a connection needs to do its job.
/// Cheap to clone via `Arc`.
pub struct ServerCtx {
    pub state: SharedState,
    pub pipeline: Arc<Pipeline>,
    pub mode: WyomingMode,
}

/// Bind every requested listener and spawn its accept loop. Returns the
/// `JoinHandle` set so the caller can keep them alive (or abort on
/// shutdown if it cares to).
pub async fn serve(addrs: Vec<ListenAddr>, ctx: Arc<ServerCtx>) -> Result<Vec<JoinHandle<()>>> {
    if !matches!(ctx.mode, WyomingMode::LocalMic) {
        // wyoming-server / hybrid land in D3. Until then, refuse to
        // boot in those modes rather than silently behave like local-mic.
        anyhow::bail!(
            "Wyoming mode {:?} is not implemented yet (only \"local-mic\" works in D2)",
            ctx.mode
        );
    }

    let mut joins = Vec::with_capacity(addrs.len());
    for addr in addrs {
        let ctx = Arc::clone(&ctx);
        let join = match addr {
            ListenAddr::Tcp(sock) => {
                let listener = TcpListener::bind(sock)
                    .await
                    .with_context(|| format!("binding Wyoming TCP listener {sock}"))?;
                tracing::info!(addr = %sock, "Wyoming TCP listening");
                tokio::spawn(accept_tcp(listener, sock, ctx))
            }
            ListenAddr::Unix(path) => {
                // Re-bind across restarts: a stale socket file is the
                // common failure mode of long-lived unix listeners.
                if path.exists() {
                    let _ = std::fs::remove_file(&path);
                }
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).with_context(|| {
                        format!(
                            "creating parent directory for unix socket {}",
                            path.display()
                        )
                    })?;
                }
                let listener = UnixListener::bind(&path)
                    .with_context(|| format!("binding Wyoming Unix listener {}", path.display()))?;
                tracing::info!(path = %path.display(), "Wyoming Unix listening");
                tokio::spawn(accept_unix(listener, path, ctx))
            }
            ListenAddr::Stdio => {
                tracing::info!("Wyoming stdio session starting");
                tokio::spawn(serve_stdio(ctx))
            }
        };
        joins.push(join);
    }
    Ok(joins)
}

async fn accept_tcp(listener: TcpListener, addr: SocketAddr, ctx: Arc<ServerCtx>) {
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let ctx = Arc::clone(&ctx);
                let peer_str = peer.to_string();
                tokio::spawn(async move {
                    let (r, w) = stream.into_split();
                    if let Err(err) = handle(BufReader::new(r), w, ctx, peer_str.clone()).await {
                        tracing::warn!(?err, peer = %peer_str, "Wyoming TCP session ended with error");
                    }
                });
            }
            Err(err) => {
                tracing::error!(?err, %addr, "TCP accept failed");
            }
        }
    }
}

async fn accept_unix(listener: UnixListener, path: PathBuf, ctx: Arc<ServerCtx>) {
    let mut conn_id: u64 = 0;
    loop {
        match listener.accept().await {
            Ok((stream, _peer)) => {
                conn_id = conn_id.wrapping_add(1);
                let ctx = Arc::clone(&ctx);
                let peer_str = format!("unix-peer-{conn_id}");
                tokio::spawn(async move {
                    let (r, w) = stream.into_split();
                    if let Err(err) = handle(BufReader::new(r), w, ctx, peer_str.clone()).await {
                        tracing::warn!(?err, peer = %peer_str, "Wyoming Unix session ended with error");
                    }
                });
            }
            Err(err) => {
                tracing::error!(?err, path = %path.display(), "Unix accept failed");
            }
        }
    }
}

async fn serve_stdio(ctx: Arc<ServerCtx>) {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    if let Err(err) = handle(BufReader::new(stdin), stdout, ctx, "stdio".into()).await {
        tracing::warn!(?err, "Wyoming stdio session ended with error");
    }
}
