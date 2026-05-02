//! Mic domain. The events it emits are the audio-domain events
//! ([`AudioStart`](crate::audio::AudioStart),
//! [`AudioChunk`](crate::audio::AudioChunk),
//! [`AudioStop`](crate::audio::AudioStop)); the only mic-specific bit
//! is the [`MicProgram`](crate::info::MicProgram) info entry.
