//! Audio source abstraction shared across the horchd ecosystem.
//!
//! Lives in `horchd-client` so subscribers (`horchctl process`, external
//! integrators) can implement and consume sources without depending on
//! the daemon binary.

use tokio::sync::mpsc;

pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// 80 ms at [`TARGET_SAMPLE_RATE`] — the openWakeWord pipeline's frame size.
pub const FRAME_SAMPLES: usize = 1280;

/// Boxed so the real-time audio callback never allocates on the stack.
pub type AudioFrame = Box<[f32; FRAME_SAMPLES]>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDescriptor {
    pub name: String,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Mic,
    File,
    Stdin,
    WyomingStream,
    Buffered,
}

/// Source of [`AudioFrame`]s.
///
/// Implementations own whatever resources back the stream and stop them
/// on drop. They may be `!Send` (e.g. `MicSource` holds a `!Send` cpal
/// stream), which propagates through `Box<dyn AudioSource>` and forces
/// the consumer loop onto the thread that started the source. The
/// returned receiver is always `Send`.
pub trait AudioSource {
    fn start(&mut self) -> anyhow::Result<mpsc::Receiver<AudioFrame>>;

    fn descriptor(&self) -> &SourceDescriptor;

    /// Reset on Wyoming `audio-start` markers and file-source replays.
    fn reset(&mut self) {}
}
