use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed header: {0}")]
    Header(serde_json::Error),
    #[error("malformed data segment: {0}")]
    Data(serde_json::Error),
    /// Header carried `payload_length` or `data_length` larger than what
    /// we're willing to allocate. Defends against hostile peers.
    #[error("frame {kind} too large: {size} bytes (cap {cap})")]
    TooLarge {
        kind: &'static str,
        size: usize,
        cap: usize,
    },
    /// `Eventable::from_event` was called on an [`Event`](crate::Event)
    /// whose `type` field doesn't match the target type's
    /// `EVENT_TYPE` constant.
    #[error("expected event type {expected:?}, got {actual:?}")]
    WrongType {
        expected: &'static str,
        actual: String,
    },
    /// The event's data didn't deserialize into the target struct.
    #[error("event {event_type:?} payload did not match the typed schema: {source}")]
    Schema {
        event_type: &'static str,
        source: serde_json::Error,
    },
    /// Header was missing a required field (`type`).
    #[error("frame header missing required field: {0}")]
    MissingField(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;
