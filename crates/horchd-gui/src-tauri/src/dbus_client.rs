//! Thin wrappers around `horchd_core::DaemonProxy`. Each call opens a
//! short-lived session-bus connection — D-Bus is cheap and avoiding
//! shared state keeps the GUI process resilient against daemon restarts.

use anyhow::{Context, Result};
use horchd_core::DaemonProxy;

pub async fn proxy() -> Result<DaemonProxy<'static>> {
    let conn = zbus::Connection::session()
        .await
        .context("connecting to the D-Bus session bus")?;
    DaemonProxy::new(&conn)
        .await
        .context("constructing horchd D-Bus proxy")
}
