use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub engine: Engine,
    #[serde(default, rename = "wakeword")]
    pub wakewords: Vec<Wakeword>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Engine {
    #[serde(default = "Engine::default_device")]
    pub device: String,
    #[serde(default = "Engine::default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "Engine::default_log_level")]
    pub log_level: String,
    pub shared_models: SharedModels,
}

impl Engine {
    pub const DEFAULT_SAMPLE_RATE: u32 = 16_000;

    fn default_device() -> String {
        "default".into()
    }
    fn default_sample_rate() -> u32 {
        Self::DEFAULT_SAMPLE_RATE
    }
    fn default_log_level() -> String {
        "info".into()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SharedModels {
    pub melspectrogram: PathBuf,
    pub embedding: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Wakeword {
    pub name: String,
    pub model: PathBuf,
    #[serde(default = "Wakeword::default_threshold")]
    pub threshold: f64,
    #[serde(default = "Wakeword::default_cooldown_ms")]
    pub cooldown_ms: u32,
    #[serde(default = "Wakeword::default_enabled")]
    pub enabled: bool,
}

impl Wakeword {
    pub const DEFAULT_THRESHOLD: f64 = 0.5;
    pub const DEFAULT_COOLDOWN_MS: u32 = 1500;
    pub const DEFAULT_ENABLED: bool = true;

    fn default_threshold() -> f64 {
        Self::DEFAULT_THRESHOLD
    }
    fn default_cooldown_ms() -> u32 {
        Self::DEFAULT_COOLDOWN_MS
    }
    fn default_enabled() -> bool {
        Self::DEFAULT_ENABLED
    }
}

impl Config {
    /// Load, expand `~`/`$VAR` paths, and validate a config file.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| Error::ConfigRead {
            path: path.to_path_buf(),
            source,
        })?;
        let mut cfg = parse(&text, path)?;
        cfg.expand_paths()?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn expand_paths(&mut self) -> Result<()> {
        expand_in_place(&mut self.engine.shared_models.melspectrogram)?;
        expand_in_place(&mut self.engine.shared_models.embedding)?;
        for wake in &mut self.wakewords {
            expand_in_place(&mut wake.model)?;
        }
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        let mut seen: HashSet<&str> = HashSet::with_capacity(self.wakewords.len());
        for wake in &self.wakewords {
            if wake.name.is_empty() {
                return Err(Error::EmptyWakewordName);
            }
            if !seen.insert(wake.name.as_str()) {
                return Err(Error::DuplicateWakeword(wake.name.clone()));
            }
        }
        Ok(())
    }
}

impl FromStr for Config {
    type Err = Error;

    /// Parse + validate a config from inline TOML. Skips `~`/`$VAR`
    /// expansion so tests stay deterministic across machines.
    fn from_str(s: &str) -> Result<Self> {
        let cfg = parse(s, Path::new("<inline>"))?;
        cfg.validate()?;
        Ok(cfg)
    }
}

fn parse(text: &str, source_path: &Path) -> Result<Config> {
    toml::from_str(text).map_err(|source| Error::ConfigParse {
        path: source_path.to_path_buf(),
        source,
    })
}

fn expand_in_place(path: &mut PathBuf) -> Result<()> {
    let raw = path.to_string_lossy().into_owned();
    let expanded = shellexpand::full(&raw).map_err(|source| Error::PathExpand {
        raw: raw.clone(),
        source,
    })?;
    *path = PathBuf::from(expanded.into_owned());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[engine]
device = "default"
sample_rate = 16000
log_level = "debug"

[engine.shared_models]
melspectrogram = "/usr/local/share/horchd/melspectrogram.onnx"
embedding = "/usr/local/share/horchd/embedding_model.onnx"

[[wakeword]]
name = "alexa"
model = "/home/user/.local/share/horchd/models/alexa.onnx"
threshold = 0.45
cooldown_ms = 1200
enabled = true

[[wakeword]]
name = "jarvis"
model = "/home/user/.local/share/horchd/models/jarvis.onnx"
"#;

    #[test]
    fn parses_full_config_with_defaults() {
        let cfg: Config = SAMPLE.parse().expect("parse");
        assert_eq!(cfg.engine.device, "default");
        assert_eq!(cfg.engine.sample_rate, 16_000);
        assert_eq!(cfg.engine.log_level, "debug");
        assert_eq!(cfg.wakewords.len(), 2);

        let alexa = &cfg.wakewords[0];
        assert_eq!(alexa.name, "alexa");
        assert!((alexa.threshold - 0.45).abs() < f64::EPSILON);
        assert_eq!(alexa.cooldown_ms, 1200);
        assert!(alexa.enabled);

        let jarvis = &cfg.wakewords[1];
        assert!((jarvis.threshold - Wakeword::DEFAULT_THRESHOLD).abs() < f64::EPSILON);
        assert_eq!(jarvis.cooldown_ms, Wakeword::DEFAULT_COOLDOWN_MS);
        assert_eq!(jarvis.enabled, Wakeword::DEFAULT_ENABLED);
    }

    #[test]
    fn round_trips_through_toml() {
        let cfg: Config = SAMPLE.parse().expect("parse");
        let serialized = toml::to_string(&cfg).expect("serialize");
        let again: Config = serialized.parse().expect("re-parse");
        assert_eq!(cfg.wakewords.len(), again.wakewords.len());
        assert_eq!(cfg.wakewords[0].name, again.wakewords[0].name);
        assert_eq!(cfg.engine.sample_rate, again.engine.sample_rate);
    }

    #[test]
    fn rejects_duplicate_wakeword_names() {
        let extra =
            format!("{SAMPLE}\n[[wakeword]]\nname = \"alexa\"\nmodel = \"/tmp/dup.onnx\"\n");
        let err = extra.parse::<Config>().unwrap_err();
        assert!(matches!(err, Error::DuplicateWakeword(name) if name == "alexa"));
    }

    #[test]
    fn rejects_empty_wakeword_name() {
        let bad = r#"
[engine]
[engine.shared_models]
melspectrogram = "/m.onnx"
embedding = "/e.onnx"

[[wakeword]]
name = ""
model = "/x.onnx"
"#;
        let err = bad.parse::<Config>().unwrap_err();
        assert!(matches!(err, Error::EmptyWakewordName));
    }

    #[test]
    fn rejects_unknown_engine_fields() {
        let bad = r#"
[engine]
device = "default"
sample_rate = 16000
log_level = "info"
unexpected = true

[engine.shared_models]
melspectrogram = "/m.onnx"
embedding = "/e.onnx"
"#;
        let err = bad.parse::<Config>().unwrap_err();
        assert!(matches!(err, Error::ConfigParse { .. }));
    }
}
