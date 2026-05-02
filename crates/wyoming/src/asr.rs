//! Speech-to-text events.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transcribe {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl_eventable!(Transcribe, "transcribe");

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    pub text: String,
}

impl_eventable!(Transcript, "transcript");
