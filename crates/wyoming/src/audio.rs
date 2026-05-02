//! Audio domain events: `audio-start`, `audio-chunk`, `audio-stop`,
//! plus the [`AudioFormat`] used by mic/snd info entries.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::event::{Event, Eventable};

/// 16 kHz mono int16 is the de-facto Wyoming standard; openWakeWord +
/// Whisper + Piper all default to it. `width` is bytes per sample, not bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioFormat {
    pub rate: u32,
    pub width: u8,
    pub channels: u8,
}

impl AudioFormat {
    pub const WAKEWORD_DEFAULT: Self = Self {
        rate: 16_000,
        width: 2,
        channels: 1,
    };
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioStart {
    pub rate: u32,
    pub width: u8,
    pub channels: u8,
    /// Microseconds since the audio source started, or `None` to mean
    /// "I don't track this".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
}

impl_eventable!(AudioStart, "audio-start");

/// PCM audio chunk. Carries raw samples in [`Event::payload`]; data
/// fields describe how to interpret them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioChunk {
    pub rate: u32,
    pub width: u8,
    pub channels: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
    /// PCM bytes, length = `samples * width * channels`.
    /// Skipped from the JSON data; rides in [`Event::payload`].
    #[serde(skip)]
    pub audio: Vec<u8>,
}

impl Eventable for AudioChunk {
    const EVENT_TYPE: &'static str = "audio-chunk";

    fn into_event(self) -> Event {
        let mut data = serde_json::Map::new();
        data.insert("rate".into(), self.rate.into());
        data.insert("width".into(), u64::from(self.width).into());
        data.insert("channels".into(), u64::from(self.channels).into());
        if let Some(ts) = self.timestamp {
            data.insert("timestamp".into(), ts.into());
        } else {
            data.insert("timestamp".into(), serde_json::Value::Null);
        }
        Event::new(Self::EVENT_TYPE)
            .with_data(data)
            .with_payload(self.audio)
    }

    fn from_event(event: &Event) -> Result<Self> {
        if event.event_type != Self::EVENT_TYPE {
            return Err(Error::WrongType {
                expected: Self::EVENT_TYPE,
                actual: event.event_type.clone(),
            });
        }
        let rate = event
            .data
            .get("rate")
            .and_then(serde_json::Value::as_u64)
            .ok_or(Error::MissingField("rate"))? as u32;
        let width = event
            .data
            .get("width")
            .and_then(serde_json::Value::as_u64)
            .ok_or(Error::MissingField("width"))? as u8;
        let channels = event
            .data
            .get("channels")
            .and_then(serde_json::Value::as_u64)
            .ok_or(Error::MissingField("channels"))? as u8;
        let timestamp = event
            .data
            .get("timestamp")
            .and_then(serde_json::Value::as_u64);
        let audio = event.payload.clone().unwrap_or_default();
        Ok(Self {
            rate,
            width,
            channels,
            timestamp,
            audio,
        })
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioStop {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
}

impl_eventable!(AudioStop, "audio-stop");
