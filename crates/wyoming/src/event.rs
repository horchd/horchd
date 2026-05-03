//! Wire-level Wyoming event: codec + [`Eventable`] trait.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{Error, Result};

/// Self-identification string put in every outgoing header. Mirrors
/// the Python wyoming library's behaviour and lets receivers log which
/// implementation produced the frame.
pub const VERSION: &str = "horchd-wyoming/0.1";

/// Hard cap on `data_length` and `payload_length`: defends against a
/// hostile peer announcing a multi-GB frame and exhausting memory
/// before we even start parsing. 16 MiB is comfortably above any
/// real-world wakeword/audio payload (one second of 48 kHz/16-bit
/// stereo PCM is ~190 KB).
pub const MAX_FRAME_SECTION: usize = 16 * 1024 * 1024;

/// Decoded Wyoming event — the wire-level frame, with all the data-
/// segment fields merged into [`data`](Event::data).
///
/// Convert to/from typed event structs via the [`Eventable`] trait.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub event_type: String,
    /// All fields from the inline header `data` plus the data-length
    /// segment, merged. Per the reference Python writer convention,
    /// outgoing events keep the inline header empty and put everything
    /// in the data-length segment.
    pub data: serde_json::Map<String, serde_json::Value>,
    pub payload: Option<Vec<u8>>,
}

impl Event {
    pub fn new(event_type: impl Into<String>) -> Self {
        Self {
            event_type: event_type.into(),
            data: serde_json::Map::new(),
            payload: None,
        }
    }

    pub fn with_data(mut self, data: serde_json::Map<String, serde_json::Value>) -> Self {
        self.data = data;
        self
    }

    pub fn with_payload(mut self, payload: Vec<u8>) -> Self {
        self.payload = Some(payload);
        self
    }
}

/// Type-safe view onto an [`Event`]: each Wyoming event type
/// (`describe`, `info`, `detect`, `audio-chunk`, …) is a Rust struct
/// that knows its wire `type` constant and how to round-trip through
/// the generic [`Event`] container.
///
/// Inspired by the trait shape in the GPL-3.0 `bryanboettcher/wyoming-rust`
/// crate (design only; no code copied — that crate's licence is
/// incompatible with horchd).
pub trait Eventable: Sized {
    const EVENT_TYPE: &'static str;

    fn into_event(self) -> Event;

    fn from_event(event: &Event) -> Result<Self>;
}

// ---------- wire codec ----------

/// Header line shape, before the `data_length`-segment is merged in.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Header {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data_length: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    payload_length: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

/// Read one Wyoming event from a buffered reader. Returns `Ok(None)`
/// on a clean EOF (end of stream between events).
pub async fn read_event<R>(reader: &mut R) -> Result<Option<Event>>
where
    R: AsyncBufRead + Unpin,
{
    let mut header_line = String::new();
    let n = reader.read_line(&mut header_line).await?;
    if n == 0 {
        return Ok(None);
    }
    // Strip the trailing `\n` (and a CR if some peer emits CRLF).
    let trimmed = header_line.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        // An empty line between frames isn't part of the spec but is
        // cheap to ignore.
        return Box::pin(read_event(reader)).await;
    }
    let header: Header = serde_json::from_str(trimmed).map_err(Error::Header)?;

    let mut merged_data = header.data.unwrap_or_default();

    if let Some(n) = header.data_length.filter(|&n| n > 0) {
        guard_size("data", n)?;
        let mut buf = vec![0u8; n];
        reader.read_exact(&mut buf).await?;
        let extra: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&buf).map_err(Error::Data)?;
        for (k, v) in extra {
            merged_data.insert(k, v);
        }
    }

    let payload = match header.payload_length.filter(|&n| n > 0) {
        Some(n) => {
            guard_size("payload", n)?;
            let mut buf = vec![0u8; n];
            reader.read_exact(&mut buf).await?;
            Some(buf)
        }
        None => None,
    };

    Ok(Some(Event {
        event_type: header.event_type,
        data: merged_data,
        payload,
    }))
}

/// Write one Wyoming event. Mirrors the reference Python writer: the
/// header line is minimal (`type`, `data_length`, `payload_length`,
/// `version`), and the actual `data` map is encoded as the
/// `data_length` segment that follows.
pub async fn write_event<W>(writer: &mut W, event: &Event) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Stable field order regardless of how the user populated `data`.
    let stable: BTreeMap<&str, &serde_json::Value> =
        event.data.iter().map(|(k, v)| (k.as_str(), v)).collect();
    let data_bytes = serde_json::to_vec(&stable).map_err(Error::Data)?;

    let header = Header {
        event_type: event.event_type.clone(),
        data: None,
        data_length: (!event.data.is_empty()).then_some(data_bytes.len()),
        payload_length: event.payload.as_ref().map(Vec::len),
        version: Some(VERSION.to_string()),
    };
    let mut header_bytes = serde_json::to_vec(&header).map_err(Error::Header)?;
    header_bytes.push(b'\n');

    writer.write_all(&header_bytes).await?;
    if !event.data.is_empty() {
        writer.write_all(&data_bytes).await?;
    }
    if let Some(payload) = event.payload.as_deref() {
        writer.write_all(payload).await?;
    }
    writer.flush().await?;
    Ok(())
}

fn guard_size(kind: &'static str, size: usize) -> Result<()> {
    if size > MAX_FRAME_SECTION {
        return Err(Error::TooLarge {
            kind,
            size,
            cap: MAX_FRAME_SECTION,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tokio::io::BufReader;

    async fn round_trip(event: Event) -> Event {
        let mut wire = Vec::new();
        write_event(&mut wire, &event).await.unwrap();
        let mut reader = BufReader::new(Cursor::new(wire));
        read_event(&mut reader).await.unwrap().unwrap()
    }

    #[tokio::test]
    async fn round_trip_describe_no_data_no_payload() {
        let evt = Event::new("describe");
        let back = round_trip(evt.clone()).await;
        assert_eq!(back.event_type, "describe");
        assert!(back.data.is_empty());
        assert!(back.payload.is_none());
    }

    #[tokio::test]
    async fn round_trip_event_with_data() {
        let mut data = serde_json::Map::new();
        data.insert("name".into(), "alexa".into());
        data.insert("score".into(), serde_json::Value::from(0.83));
        let evt = Event::new("detection").with_data(data);
        let back = round_trip(evt.clone()).await;
        assert_eq!(back.event_type, "detection");
        assert_eq!(back.data.get("name").unwrap(), "alexa");
        assert_eq!(back.data.get("score").unwrap().as_f64(), Some(0.83));
    }

    #[tokio::test]
    async fn round_trip_event_with_payload() {
        let evt = Event::new("audio-chunk").with_payload(vec![1, 2, 3, 4, 5, 6, 7, 8]);
        let back = round_trip(evt.clone()).await;
        assert_eq!(
            back.payload.as_deref(),
            Some(&[1u8, 2, 3, 4, 5, 6, 7, 8][..])
        );
    }

    #[tokio::test]
    async fn read_returns_none_on_clean_eof() {
        let mut reader = BufReader::new(Cursor::new(Vec::<u8>::new()));
        assert!(read_event(&mut reader).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn rejects_oversized_data_length() {
        // Manually craft a header that announces a 32-MiB data segment.
        let mut wire = Vec::new();
        wire.extend_from_slice(br#"{"type":"x","data_length":33554432}"#.as_ref());
        wire.push(b'\n');
        let mut reader = BufReader::new(Cursor::new(wire));
        let err = read_event(&mut reader).await.unwrap_err();
        assert!(matches!(err, Error::TooLarge { kind: "data", .. }));
    }

    #[tokio::test]
    async fn header_data_inline_merges_into_event_data() {
        // Some senders put `data` inline in the header (legacy form).
        // The reader should still surface it on `Event.data`.
        let mut wire = Vec::new();
        wire.extend_from_slice(
            br#"{"type":"detection","data":{"name":"alexa","score":0.9}}"#.as_ref(),
        );
        wire.push(b'\n');
        let mut reader = BufReader::new(Cursor::new(wire));
        let evt = read_event(&mut reader).await.unwrap().unwrap();
        assert_eq!(evt.event_type, "detection");
        assert_eq!(evt.data.get("name").unwrap(), "alexa");
    }

    #[tokio::test]
    async fn header_data_length_segment_overrides_inline() {
        // If both inline `data` and a `data_length` segment are present,
        // the segment values override (Python: `dict.update()`).
        let body = br#"{"name":"alexa","score":0.99}"#;
        let mut wire = Vec::new();
        let header = format!(
            r#"{{"type":"detection","data":{{"score":0.5}},"data_length":{}}}"#,
            body.len()
        );
        wire.extend_from_slice(header.as_bytes());
        wire.push(b'\n');
        wire.extend_from_slice(body);
        let mut reader = BufReader::new(Cursor::new(wire));
        let evt = read_event(&mut reader).await.unwrap().unwrap();
        assert_eq!(evt.data.get("name").unwrap(), "alexa");
        assert_eq!(evt.data.get("score").unwrap().as_f64(), Some(0.99));
    }
}
