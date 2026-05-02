//! Wire-compat tests against the reference Python `wyoming` library
//! shape.
//!
//! The byte literals below mirror what `wyoming.event.Event.write` and
//! `wyoming.client.AsyncClient.write_event` produce on the wire. The
//! spec we follow:
//!
//! - Header is minimal: `type`, `data_length`, `payload_length`,
//!   `version`. Inline `data` is left out by the writer (but tolerated
//!   on read).
//! - All real data lives in the `data_length`-segment, encoded as
//!   compact JSON (no extra whitespace).
//! - Version string mirrors the Python lib's `wyoming.VERSION`.
//!
//! These tests capture two concerns:
//!
//! 1. **Decode**: bytes that look like what Python wyoming would emit
//!    parse correctly into our typed event structs.
//! 2. **Encode**: round-tripping through our writer produces an
//!    equivalent decode (we don't byte-compare against Python — JSON
//!    field ordering can legitimately differ — but we assert the
//!    re-decoded `Event` matches the original).

use std::io::Cursor;

use horchd_wyoming::audio::{AudioChunk, AudioFormat, AudioStart, AudioStop};
use horchd_wyoming::event::{Event, Eventable, read_event, write_event};
use horchd_wyoming::info::{Attribution, Info, WakeModel, WakeProgram};
use horchd_wyoming::wake::{Detect, Detection, NotDetected};
use tokio::io::BufReader;

/// Decode a complete wyoming wire frame buffer into a single Event.
async fn decode(bytes: &[u8]) -> Event {
    let mut reader = BufReader::new(Cursor::new(bytes.to_vec()));
    read_event(&mut reader)
        .await
        .expect("read_event")
        .expect("non-empty stream")
}

/// Build a Python-shape wire frame: minimal header + data segment +
/// optional binary payload. Auto-computes the lengths so test authors
/// don't have to hand-count UTF-8 byte counts.
fn python_shape(event_type: &str, data: &[u8], payload: Option<&[u8]>) -> Vec<u8> {
    use std::fmt::Write as _;
    let mut header = format!("{{\"type\":\"{event_type}\",\"data_length\":{}", data.len());
    if let Some(p) = payload {
        let _ = write!(header, ",\"payload_length\":{}", p.len());
    }
    header.push_str(",\"version\":\"1.8.0\"}\n");
    let mut wire = header.into_bytes();
    wire.extend_from_slice(data);
    if let Some(p) = payload {
        wire.extend_from_slice(p);
    }
    wire
}

/// Round-trip via our writer + reader.
async fn round_trip<E: Eventable + Clone + std::fmt::Debug + PartialEq>(typed: E) -> E {
    let mut wire = Vec::new();
    write_event(&mut wire, &typed.clone().into_event())
        .await
        .expect("write_event");
    let event = decode(&wire).await;
    E::from_event(&event).expect("from_event")
}

#[tokio::test]
async fn decodes_python_describe_frame() {
    // Python `wyoming.event.Describe()` writes "{}" in the length
    // segment even when the data is empty. Our reader merges it back
    // into an empty map.
    let event = decode(&python_shape("describe", b"{}", None)).await;
    assert_eq!(event.event_type, "describe");
    assert!(event.data.is_empty());
    assert!(event.payload.is_none());
}

#[tokio::test]
async fn decodes_python_detect_frame() {
    let event = decode(&python_shape("detect", br#"{"names":["alexa"]}"#, None)).await;
    let detect = Detect::from_event(&event).expect("Detect schema");
    assert_eq!(detect.names, vec!["alexa".to_string()]);
}

#[tokio::test]
async fn decodes_python_detection_frame() {
    let event = decode(&python_shape(
        "detection",
        br#"{"name":"alexa","timestamp":1234}"#,
        None,
    ))
    .await;
    let det = Detection::from_event(&event).expect("Detection schema");
    assert_eq!(det.name, "alexa");
    assert_eq!(det.timestamp, Some(1234));
}

#[tokio::test]
async fn decodes_python_not_detected_frame() {
    let event = decode(&python_shape("not-detected", b"{}", None)).await;
    let _: NotDetected = NotDetected::from_event(&event).expect("NotDetected schema");
}

#[tokio::test]
async fn decodes_python_audio_chunk_frame() {
    let data = br#"{"rate":16000,"width":2,"channels":1,"timestamp":null}"#;
    let payload = [0x00, 0x10, 0x00, 0x10, 0x00, 0x10, 0x00, 0x10];
    let event = decode(&python_shape("audio-chunk", data, Some(&payload))).await;
    let chunk = AudioChunk::from_event(&event).expect("AudioChunk schema");
    assert_eq!(chunk.rate, 16_000);
    assert_eq!(chunk.width, 2);
    assert_eq!(chunk.channels, 1);
    assert_eq!(chunk.timestamp, None);
    assert_eq!(chunk.audio.as_slice(), payload.as_slice());
}

#[tokio::test]
async fn round_trips_describe() {
    let original = horchd_wyoming::info::Describe::default();
    let back = round_trip(original.clone()).await;
    assert_eq!(back, original);
}

#[tokio::test]
async fn round_trips_info_with_wake_program() {
    let original = Info {
        wake: vec![WakeProgram {
            name: "horchd".into(),
            attribution: Attribution {
                name: "NewtTheWolf".into(),
                url: "https://horchd.xyz".into(),
            },
            installed: true,
            description: Some("Native multi-wakeword detection daemon".into()),
            version: Some("0.2.0".into()),
            models: vec![WakeModel {
                name: "alexa".into(),
                attribution: Attribution {
                    name: "openWakeWord".into(),
                    url: "https://github.com/dscripka/openWakeWord".into(),
                },
                installed: true,
                description: None,
                version: None,
                languages: vec!["en".into()],
                phrase: Some("Alexa".into()),
            }],
        }],
        ..Info::default()
    };
    let back = round_trip(original.clone()).await;
    assert_eq!(back, original);
}

#[tokio::test]
async fn round_trips_audio_session() {
    let start = AudioStart {
        rate: 16_000,
        width: 2,
        channels: 1,
        timestamp: Some(0),
    };
    assert_eq!(round_trip(start.clone()).await, start);

    let chunk = AudioChunk {
        rate: 16_000,
        width: 2,
        channels: 1,
        timestamp: Some(80_000),
        audio: vec![0xAA; 1280 * 2],
    };
    assert_eq!(round_trip(chunk.clone()).await, chunk);

    let stop = AudioStop {
        timestamp: Some(160_000),
    };
    assert_eq!(round_trip(stop.clone()).await, stop);
}

#[tokio::test]
async fn round_trips_detection() {
    let det = Detection {
        name: "alexa".into(),
        timestamp: Some(2_345),
        speaker: None,
    };
    assert_eq!(round_trip(det.clone()).await, det);
}

#[tokio::test]
async fn audio_format_constant_matches_wakeword_default() {
    let f = AudioFormat::WAKEWORD_DEFAULT;
    assert_eq!(f.rate, 16_000);
    assert_eq!(f.width, 2);
    assert_eq!(f.channels, 1);
}

#[tokio::test]
async fn wrong_event_type_errors_on_from_event() {
    let evt = Event::new("not-a-detect");
    let err = Detect::from_event(&evt).unwrap_err();
    assert!(matches!(
        err,
        horchd_wyoming::Error::WrongType {
            expected: "detect",
            ..
        }
    ));
}

#[tokio::test]
async fn streams_two_events_back_to_back() {
    // Python wyoming concatenates frames on the same connection. Our
    // reader must handle reading one event and leaving the buffer in a
    // state where a second read picks up the next event.
    let mut wire = Vec::new();
    write_event(&mut wire, &Detect::default().into_event())
        .await
        .unwrap();
    write_event(
        &mut wire,
        &Detection {
            name: "alexa".into(),
            timestamp: Some(0),
            speaker: None,
        }
        .into_event(),
    )
    .await
    .unwrap();

    let mut reader = BufReader::new(Cursor::new(wire));
    let first = read_event(&mut reader).await.unwrap().unwrap();
    let second = read_event(&mut reader).await.unwrap().unwrap();
    assert_eq!(first.event_type, "detect");
    assert_eq!(second.event_type, "detection");
    let third = read_event(&mut reader).await.unwrap();
    assert!(third.is_none(), "third read should be EOF");
}
