//! Timer events (for HA assist's "set a timer" feature).

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimerStarted {
    pub id: String,
    pub total_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl_eventable!(TimerStarted, "timer-started");

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimerCancelled {
    pub id: String,
}

impl_eventable!(TimerCancelled, "timer-cancelled");

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimerUpdated {
    pub id: String,
    pub total_seconds: u64,
}

impl_eventable!(TimerUpdated, "timer-updated");

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimerFinished {
    pub id: String,
}

impl_eventable!(TimerFinished, "timer-finished");
