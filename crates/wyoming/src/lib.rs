//! Wyoming protocol primitives.
//!
//! Wyoming is a JSONL-over-stream protocol used by Home Assistant's
//! voice pipeline to talk to wakeword / ASR / TTS / VAD / intent
//! services. Spec lives in <https://github.com/OHF-Voice/wyoming>; this
//! crate is a Rust port of the wire-level pieces (frame codec + event
//! types + info tree) carved out of horchd so the same primitives can
//! be reused by external integrators.
//!
//! # Frame layout
//!
//! Each Wyoming frame is up to three sections concatenated on the
//! stream:
//!
//! 1. A JSON header line (UTF-8) terminated by a single `\n`. Carries
//!    the event `type`, optional inline `data`, optional length fields
//!    `data_length` / `payload_length`, and the writer's `version`.
//! 2. Optional `data_length` bytes of additional JSON, merged into the
//!    header `data`. The reference Python writer puts ALL data here
//!    and keeps the header minimal.
//! 3. Optional `payload_length` bytes of binary payload (typically PCM).
//!
//! See [`event::read_event`] / [`event::write_event`] for the codec.
//!
//! # Domain coverage
//!
//! horchd cares primarily about the wake domain ([`wake`]), but the
//! crate ships the full set of Wyoming domains as Rust types so it can
//! act as a Rust-side reference impl on crates.io for ASR/TTS service
//! authors.

#[macro_use]
mod macros;

pub mod asr;
pub mod audio;
pub mod error;
pub mod event;
pub mod handle;
pub mod info;
pub mod intent;
pub mod mic;
pub mod snd;
pub mod timer;
pub mod tts;
pub mod wake;

pub use error::{Error, Result};
pub use event::{Event, Eventable, read_event, write_event};
