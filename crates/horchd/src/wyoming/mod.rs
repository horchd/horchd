//! Wyoming-protocol server embedded in the daemon.
//!
//! Lives inside `horchd` (not as its own crate) because the server needs
//! direct access to the live `Pipeline` and `SharedState` — splitting it
//! out would force re-exporting half of the daemon's internals through a
//! public surface no external consumer needs. The wire-level codec lives
//! in the standalone `horchd-wyoming` crate; this module is the listener,
//! the per-connection state machine, and the `Info` builder.

pub mod handler;
pub mod info;
pub mod listener;
pub mod uri;
pub mod zeroconf;

pub use listener::{ServerCtx, serve};
pub use uri::{ListenAddr, parse as parse_uri};
pub use zeroconf::{ZeroconfHandle, announce as announce_zeroconf};
