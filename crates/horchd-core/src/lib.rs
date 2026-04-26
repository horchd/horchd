//! Shared types and D-Bus interface for the horchd ecosystem.
//!
//! Consumed by `horchd` (server side: owns inference + config file) and
//! by `horchctl` / `horchd-gui` (client side: drive the daemon over the
//! session bus).

pub mod config;
pub mod dbus;
pub mod error;
pub mod event;

pub use config::{Config, Engine, SharedModels, Wakeword};
pub use dbus::{DaemonProxy, WakewordSnapshot};
pub use error::{Error, Result};
pub use event::WakewordEvent;
