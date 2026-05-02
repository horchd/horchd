//! Sink implementations for [`horchd_client::DetectionSink`].
//!
//! Each sink consumes Detection / ScoreSnapshot events from a Pipeline
//! and forwards them to a transport (D-Bus today; Wyoming and others
//! land in later phases).

pub mod dbus;

pub use dbus::DBusSink;
