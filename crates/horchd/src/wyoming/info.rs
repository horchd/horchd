//! Build the Wyoming `info` payload from the daemon's current state.
//!
//! The `info` event advertises horchd as a `wake` Program containing one
//! `WakeModel` per registered wakeword. Other Wyoming domains (asr, tts,
//! intent, …) stay empty since horchd doesn't speak them.

use horchd_wyoming::info::{Attribution, Info, WakeModel, WakeProgram};

use crate::state::DaemonState;

const HORCHD_PROGRAM_URL: &str = "https://horchd.xyz";
const HORCHD_PROGRAM_NAME: &str = "NewtTheWolf";
const OWW_ATTRIBUTION_NAME: &str = "openWakeWord";
const OWW_ATTRIBUTION_URL: &str = "https://github.com/dscripka/openWakeWord";

pub fn build_info(state: &DaemonState) -> Info {
    let models: Vec<WakeModel> = state
        .config
        .wakewords
        .iter()
        .map(|w| WakeModel {
            name: w.name.clone(),
            attribution: Attribution {
                name: OWW_ATTRIBUTION_NAME.into(),
                url: OWW_ATTRIBUTION_URL.into(),
            },
            installed: true,
            description: None,
            version: None,
            languages: Vec::new(),
            phrase: None,
        })
        .collect();

    let program = WakeProgram {
        name: "horchd".into(),
        attribution: Attribution {
            name: HORCHD_PROGRAM_NAME.into(),
            url: HORCHD_PROGRAM_URL.into(),
        },
        installed: true,
        description: Some(env!("CARGO_PKG_DESCRIPTION").into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        models,
    };

    Info {
        wake: vec![program],
        ..Info::default()
    }
}
