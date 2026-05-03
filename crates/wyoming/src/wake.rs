//! Wake domain events.

use serde::{Deserialize, Serialize};

/// Tell the server which wakewords this client cares about. Empty list
/// means "use the server's default model".
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detect {
    #[serde(default)]
    pub names: Vec<String>,
}

impl_eventable!(Detect, "detect");

/// Server says: a wakeword fired.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detection {
    pub name: String,
    /// Milliseconds since the matching `audio-start`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
    /// Some servers (incl. Wyoming reference) report the classifier
    /// score; not part of the original spec but tolerated by HA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
}

impl_eventable!(Detection, "detection");

/// Server says: this audio session ended without a wakeword fire. Sent
/// at most once per session, after the matching `audio-stop`.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotDetected {}

impl_eventable!(NotDetected, "not-detected");
