//! Live cpal microphone capture as an [`AudioSource`].
//!
//! The cpal `Stream` is `!Send`, so `MicSource` and any pipeline that
//! owns one must stay on the thread that constructed it (the main
//! thread under `#[tokio::main]`). The receiver returned by `start` is
//! `Send` and moves freely into a tokio task.

use std::sync::Arc;

use anyhow::Result;
use cpal::Stream;
use horchd_client::{AudioFrame, AudioSource, SourceDescriptor, SourceKind};
use tokio::sync::mpsc;

use super::{AudioStats, open_input_stream};

pub struct MicSource {
    device_name: String,
    channel_capacity: usize,
    stats: Arc<AudioStats>,
    descriptor: SourceDescriptor,
    /// `!Send` cpal stream — kept alive for the lifetime of the source.
    stream: Option<Stream>,
}

impl MicSource {
    pub fn new(device_name: String, channel_capacity: usize, stats: Arc<AudioStats>) -> Self {
        let descriptor = SourceDescriptor {
            name: device_name.clone(),
            kind: SourceKind::Mic,
        };
        Self {
            device_name,
            channel_capacity,
            stats,
            descriptor,
            stream: None,
        }
    }
}

impl AudioSource for MicSource {
    fn start(&mut self) -> Result<mpsc::Receiver<AudioFrame>> {
        let (stream, label, rx) = open_input_stream(
            &self.device_name,
            self.channel_capacity,
            Arc::clone(&self.stats),
        )?;
        // Surface the cpal-resolved label rather than the user's request
        // (e.g. `"default"` becomes the host's actual device name).
        self.descriptor.name = label;
        self.stream = Some(stream);
        Ok(rx)
    }

    fn descriptor(&self) -> &SourceDescriptor {
        &self.descriptor
    }
}
