//! Intent / text handling events.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Handled {
    pub text: String,
}

impl_eventable!(Handled, "handled");

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotHandled {
    pub text: String,
}

impl_eventable!(NotHandled, "not-handled");
