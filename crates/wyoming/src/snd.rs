//! Sound output events. Plays back audio-domain chunks; emits `played`
//! when the entire stream has finished playing.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Played {}

impl_eventable!(Played, "played");
