//! Shared mutable state behind a single `tokio::sync::Mutex`.
//!
//! The inference task and every `service::Daemon` D-Bus method lock the
//! same `DaemonState` to read or mutate the configured wakewords. The
//! lock is held briefly per audio frame (~ a few ms during ONNX runs);
//! mutator methods (Add/Remove/SetThreshold/Reload/…) wait their turn
//! between frames.

use std::path::PathBuf;
use std::sync::Arc;

use horchd_core::Config;
use tokio::sync::Mutex;

use crate::detector::Detector;
use crate::inference::InferencePipeline;

pub struct DaemonState {
    pub config: Config,
    pub config_path: PathBuf,
    pub pipeline: InferencePipeline,
    pub detectors: Vec<Detector>,
}

pub type SharedState = Arc<Mutex<DaemonState>>;

impl DaemonState {
    pub fn new(
        config: Config,
        config_path: PathBuf,
        pipeline: InferencePipeline,
        detectors: Vec<Detector>,
    ) -> SharedState {
        Arc::new(Mutex::new(Self {
            config,
            config_path,
            pipeline,
            detectors,
        }))
    }

    pub fn detector_mut(&mut self, name: &str) -> Option<&mut Detector> {
        self.detectors.iter_mut().find(|d| d.name == name)
    }

    pub fn wakeword_config_mut(&mut self, name: &str) -> Option<&mut horchd_core::Wakeword> {
        self.config.wakewords.iter_mut().find(|w| w.name == name)
    }
}
