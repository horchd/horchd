//! Internals of the horchd daemon, exposed as a library so benches and
//! integration tests can reach `audio` / `detector` / `inference` /
//! `persist` / `service` / `state` without re-implementing them. The
//! installed binary in `src/main.rs` depends on this library.

use tokio::sync::oneshot;

pub mod audio;
pub mod detector;
pub mod inference;
pub mod persist;
pub mod service;
pub mod state;

/// Commands the D-Bus service handler can send back to `main` so audio
/// device hot-swaps run on the thread that owns the (`!Send`) cpal
/// `Stream`. Lives at the lib root so [`service::Daemon`] can reach it
/// via `crate::AudioCmd`.
pub enum AudioCmd {
    List {
        reply: oneshot::Sender<anyhow::Result<Vec<String>>>,
    },
    SetDevice {
        name: String,
        persist: bool,
        reply: oneshot::Sender<anyhow::Result<()>>,
    },
}

/// Convenience aliases for benchmarks. Re-exports keep the public
/// surface tight: only what benches genuinely need.
pub mod detector_for_bench {
    pub use crate::detector::Detector;
}

pub mod audio_for_bench {
    pub use crate::audio::{AudioStats, CallbackState, FRAME_SAMPLES, Frame, TARGET_SAMPLE_RATE};
}
