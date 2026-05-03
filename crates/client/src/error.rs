use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("failed to read config at {path}")]
    ConfigRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse config at {path}")]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("failed to expand path {raw:?}")]
    PathExpand {
        raw: String,
        #[source]
        source: shellexpand::LookupError<std::env::VarError>,
    },

    #[error("duplicate wakeword name {0:?}")]
    DuplicateWakeword(String),

    #[error("wakeword name must not be empty")]
    EmptyWakewordName,

    #[error(
        "sample_rate must be exactly 16000 (got {got}); horchd's pipeline is hard-pinned to 16 kHz"
    )]
    InvalidSampleRate { got: u32 },

    #[error("threshold for wakeword {name:?} must be in (0, 1]; got {got}")]
    InvalidThreshold { name: String, got: f64 },

    #[error("cooldown_ms for wakeword {name:?} must be ≤ {max} ms; got {got}")]
    InvalidCooldownMs { name: String, got: u32, max: u32 },

    #[error("model path for wakeword {name:?} is not valid UTF-8")]
    NonUtf8ModelPath { name: String },

    #[error(
        "[wyoming].mode = \"local-mic\" requires [engine].local_mic = true (no local audio source otherwise)"
    )]
    WyomingLocalMicRequiresMic,
}
