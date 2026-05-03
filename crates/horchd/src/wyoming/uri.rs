//! Parse Wyoming listen URIs into the concrete socket flavour.
//!
//! Supported schemes:
//! - `tcp://HOST:PORT` (e.g. `tcp://0.0.0.0:10400`, `tcp://[::]:10400`)
//! - `unix:///abs/path/to/socket` (absolute path required)
//! - `stdio://` — single connection over stdin/stdout, useful for
//!   `socat` adapters and one-off tests

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub enum ListenAddr {
    Tcp(SocketAddr),
    Unix(PathBuf),
    Stdio,
}

pub fn parse(uri: &str) -> Result<ListenAddr> {
    if let Some(rest) = uri.strip_prefix("tcp://") {
        let addr: SocketAddr = rest
            .parse()
            .with_context(|| format!("invalid tcp:// host:port {rest:?}"))?;
        Ok(ListenAddr::Tcp(addr))
    } else if let Some(rest) = uri.strip_prefix("unix://") {
        let path = PathBuf::from(rest);
        if !path.is_absolute() {
            bail!("unix:// path must be absolute (got {rest:?})");
        }
        Ok(ListenAddr::Unix(path))
    } else if uri == "stdio://" {
        Ok(ListenAddr::Stdio)
    } else {
        bail!("unsupported Wyoming listen URI {uri:?}; expected tcp:// | unix:// | stdio://")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tcp_v4() {
        let a = parse("tcp://0.0.0.0:10400").unwrap();
        assert!(matches!(a, ListenAddr::Tcp(s) if s.port() == 10400));
    }

    #[test]
    fn parses_tcp_v6() {
        let a = parse("tcp://[::1]:10400").unwrap();
        assert!(matches!(a, ListenAddr::Tcp(s) if s.port() == 10400));
    }

    #[test]
    fn parses_unix_abs() {
        let a = parse("unix:///run/horchd/wyoming.sock").unwrap();
        assert!(matches!(a, ListenAddr::Unix(p) if p.as_os_str() == "/run/horchd/wyoming.sock"));
    }

    #[test]
    fn rejects_unix_relative() {
        assert!(parse("unix://relative/path.sock").is_err());
    }

    #[test]
    fn parses_stdio() {
        assert!(matches!(parse("stdio://").unwrap(), ListenAddr::Stdio));
    }

    #[test]
    fn rejects_unknown_scheme() {
        assert!(parse("ws://example.com:80").is_err());
    }
}
