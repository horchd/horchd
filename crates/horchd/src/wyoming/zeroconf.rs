//! Advertise the running Wyoming TCP listeners over mDNS so clients
//! (Home Assistant, `wyoming-cli`, …) can auto-discover the daemon
//! without anyone having to type an IP.
//!
//! Service type is the same as upstream Wyoming
//! (`_wyoming._tcp.local.`); HA's Wyoming integration listens for it
//! out of the box.
//!
//! Stdio and Unix listeners are deliberately *not* announced — they
//! aren't reachable over the LAN and announcing them would just confuse
//! discoverers.

use std::net::{IpAddr, SocketAddr};

use anyhow::{Context, Result};
use mdns_sd::{ServiceDaemon, ServiceInfo};

use crate::wyoming::uri::ListenAddr;

const SERVICE_TYPE: &str = "_wyoming._tcp.local.";

/// Take ownership of the running [`ServiceDaemon`] so the caller can
/// keep it alive for the daemon's lifetime — when this is dropped the
/// service is unregistered and disappears from the network.
pub struct ZeroconfHandle {
    _daemon: ServiceDaemon,
}

/// Register every TCP listener as a Wyoming service. Non-TCP listeners
/// are silently ignored. `service_name` is the human label that ends up
/// in the discovered record (`<service_name>._wyoming._tcp.local.`).
pub fn announce(addrs: &[ListenAddr], service_name: &str) -> Result<Option<ZeroconfHandle>> {
    let tcp_addrs: Vec<SocketAddr> = addrs
        .iter()
        .filter_map(|a| match a {
            ListenAddr::Tcp(s) => Some(*s),
            ListenAddr::Unix(_) | ListenAddr::Stdio => None,
        })
        .collect();
    if tcp_addrs.is_empty() {
        return Ok(None);
    }

    let host = hostname::get()
        .context("reading hostname for mDNS")?
        .to_string_lossy()
        .into_owned();
    let host_fqdn = format!("{host}.local.");

    let daemon = ServiceDaemon::new().context("starting mDNS service daemon")?;

    for addr in &tcp_addrs {
        let local_ips = local_ip_candidates(addr.ip())?;
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            service_name,
            &host_fqdn,
            local_ips.as_slice(),
            addr.port(),
            None,
        )
        .context("building mDNS ServiceInfo")?;
        daemon
            .register(info)
            .with_context(|| format!("registering mDNS service for {addr}"))?;
        tracing::info!(
            service = service_name,
            addr = %addr,
            "Wyoming mDNS announced"
        );
    }

    Ok(Some(ZeroconfHandle { _daemon: daemon }))
}

/// `0.0.0.0` / `[::]` mean "any interface" — mdns-sd needs concrete
/// addresses to announce. Enumerate every non-loopback interface we
/// can find. For an explicit bind address we just use it as-is.
fn local_ip_candidates(bind: IpAddr) -> Result<Vec<IpAddr>> {
    if !bind.is_unspecified() {
        return Ok(vec![bind]);
    }
    let want_v4 = bind.is_ipv4();
    let mut out: Vec<IpAddr> = if_addrs::get_if_addrs()
        .context("enumerating local network interfaces")?
        .into_iter()
        .filter(|ifa| !ifa.is_loopback())
        .map(|ifa| ifa.ip())
        .filter(|ip| ip.is_ipv4() == want_v4 || !want_v4)
        .collect();
    if out.is_empty() {
        // Fall back to loopback so we at least announce *something*;
        // a developer running `horchd` locally still wants the service
        // to be discoverable from the same machine.
        out.push(if want_v4 {
            IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        } else {
            IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
        });
    }
    Ok(out)
}
