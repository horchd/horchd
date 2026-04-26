use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
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
}
