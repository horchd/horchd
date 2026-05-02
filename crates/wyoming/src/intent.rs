//! Intent recognition events.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recognize {
    pub text: String,
}

impl_eventable!(Recognize, "recognize");

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Intent {
    pub name: String,
    #[serde(default)]
    pub entities: BTreeMap<String, serde_json::Value>,
}

impl_eventable!(Intent, "intent");

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotRecognized {
    pub text: String,
}

impl_eventable!(NotRecognized, "not-recognized");
