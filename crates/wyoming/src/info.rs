//! `describe` / `info` events + the artifact tree (Program/Model/Voice/…).

use serde::{Deserialize, Serialize};

/// `{"type":"describe"}` — request the peer's capability advertisement.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Describe {}

impl_eventable!(Describe, "describe");

/// `{"type":"info"}` — capability advertisement. All domain lists are
/// always present; populate only the ones your service implements.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Info {
    #[serde(default)]
    pub asr: Vec<AsrProgram>,
    #[serde(default)]
    pub tts: Vec<TtsProgram>,
    #[serde(default)]
    pub handle: Vec<HandleProgram>,
    #[serde(default)]
    pub intent: Vec<IntentProgram>,
    #[serde(default)]
    pub wake: Vec<WakeProgram>,
    #[serde(default)]
    pub mic: Vec<MicProgram>,
    #[serde(default)]
    pub snd: Vec<SndProgram>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub satellite: Option<Satellite>,
}

impl_eventable!(Info, "info");

/// Where a model / voice / program comes from. Required on every artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attribution {
    pub name: String,
    pub url: String,
}

// ---------- per-domain artifact structs ----------
//
// Wyoming artifacts share five fields: name, attribution, installed,
// description?, version?. The `define_artifact!` macro below stamps
// out the boilerplate; per-domain extras are listed in the macro call.

/// Generates a struct with the canonical Wyoming-artifact fields plus
/// any extras passed in `{ ... }`. Extra fields can carry `#[serde(...)]`
/// attributes.
#[macro_export]
macro_rules! define_artifact {
    (
        $(#[$meta:meta])*
        $vis:vis $name:ident {
            $(
                $(#[$field_meta:meta])*
                $extra_vis:vis $extra_field:ident: $extra_ty:ty
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
        $vis struct $name {
            pub name: String,
            pub attribution: $crate::info::Attribution,
            pub installed: bool,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub description: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub version: Option<String>,
            $(
                $(#[$field_meta])*
                $extra_vis $extra_field: $extra_ty,
            )*
        }
    };
}

// Wake — the domain horchd serves natively.

define_artifact! {
    /// One wakeword classifier exposed by a [`WakeProgram`].
    pub WakeModel {
        /// BCP-47 language tags the model was trained for.
        #[serde(default)]
        pub languages: Vec<String>,
        /// Spoken phrase the model triggers on (`"Alexa"`, `"hey jarvis"`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub phrase: Option<String>,
    }
}

define_artifact! {
    /// A wakeword detection service (`"horchd"`, `"openwakeword"`).
    pub WakeProgram {
        pub models: Vec<WakeModel>,
    }
}

// ASR (speech-to-text)

define_artifact! {
    pub AsrModel {
        #[serde(default)]
        pub languages: Vec<String>,
    }
}

define_artifact! {
    pub AsrProgram {
        pub models: Vec<AsrModel>,
    }
}

// TTS (text-to-speech)

/// Per-speaker voice handle. Not a full artifact — just the name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TtsVoiceSpeaker {
    pub name: String,
}

define_artifact! {
    pub TtsVoice {
        #[serde(default)]
        pub languages: Vec<String>,
        #[serde(default)]
        pub speakers: Vec<TtsVoiceSpeaker>,
    }
}

define_artifact! {
    pub TtsProgram {
        pub voices: Vec<TtsVoice>,
    }
}

// Intent / Handle

define_artifact! {
    pub IntentProgram {
        #[serde(default)]
        pub languages: Vec<String>,
    }
}

define_artifact! {
    pub HandleProgram {
        #[serde(default)]
        pub languages: Vec<String>,
    }
}

// Mic / Snd

define_artifact! {
    pub MicProgram {
        pub mic_format: crate::audio::AudioFormat,
    }
}

define_artifact! {
    pub SndProgram {
        pub snd_format: crate::audio::AudioFormat,
    }
}

// Satellite (optional, single instance)

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Satellite {
    pub name: String,
    pub attribution: Attribution,
    pub installed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Wakeword names this satellite is currently listening for.
    #[serde(default)]
    pub active_wake_words: Vec<String>,
}
