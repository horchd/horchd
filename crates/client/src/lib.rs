//! Shared types and D-Bus interface for the horchd ecosystem.
//!
//! Consumed by `horchd` (server side: owns inference + config file) and
//! by `horchctl` / `horchd-gui` (client side: drive the daemon over the
//! session bus). The [`audio`] and [`sink`] modules expose the
//! `AudioSource` / `DetectionSink` traits that let external integrators
//! plug in alternative inputs (file, stream, Wyoming) and outputs
//! (Wyoming, custom transports).

pub mod audio;
pub mod config;
pub mod dbus;
pub mod error;
pub mod sink;

pub use audio::{
    AudioFrame, AudioSource, FRAME_SAMPLES, SourceDescriptor, SourceKind, TARGET_SAMPLE_RATE,
};
pub use config::{
    Config, Engine, MAX_COOLDOWN_MS, SharedModels, Wakeword, WyomingConfig, WyomingMode,
};
pub use dbus::{DaemonProxy, DetectionEntry, WakewordSnapshot};
pub use error::{Error, Result};
pub use sink::{Detection, DetectionSink, ScoreSnapshot};
