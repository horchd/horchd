use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Hard cap on `cooldown_ms` (60 s). Anything longer is almost certainly
/// a misconfiguration (a typo of "60_000" → "600_000", say) — past this
/// the wakeword effectively never fires.
pub const MAX_COOLDOWN_MS: u32 = 60_000;

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
    /// Load, expand `~` paths, and validate a config file.
    ///
    /// `$VAR` env-var expansion is intentionally NOT performed — only
    /// tilde — so a hostile env (or a sudo-stripped env) cannot redirect
    /// a model path. Callers that need env vars in their config can use
    /// `XDG_DATA_HOME` semantics from the calling layer instead.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| Error::ConfigRead {
            path: path.to_path_buf(),
            source,
        })?;
        let mut cfg = parse(&text, path)?;
        cfg.expand_paths();
        cfg.validate()?;
        Ok(cfg)
    }

    fn expand_paths(&mut self) {
        expand_in_place(&mut self.engine.shared_models.melspectrogram);
        expand_in_place(&mut self.engine.shared_models.embedding);
        for wake in &mut self.wakewords {
            expand_in_place(&mut wake.model);
        }
    }

    fn validate(&self) -> Result<()> {
        if self.engine.sample_rate != Engine::DEFAULT_SAMPLE_RATE {
            return Err(Error::InvalidSampleRate {
                got: self.engine.sample_rate,
            });
        }
        let mut seen: HashSet<&str> = HashSet::with_capacity(self.wakewords.len());
        for wake in &self.wakewords {
            if wake.name.is_empty() {
                return Err(Error::EmptyWakewordName);
            }
            if !seen.insert(wake.name.as_str()) {
                return Err(Error::DuplicateWakeword(wake.name.clone()));
            }
            if !(wake.threshold > 0.0 && wake.threshold <= 1.0) {
                return Err(Error::InvalidThreshold {
                    name: wake.name.clone(),
                    got: wake.threshold,
                });
            }
            if wake.cooldown_ms > MAX_COOLDOWN_MS {
                return Err(Error::InvalidCooldownMs {
                    name: wake.name.clone(),
                    got: wake.cooldown_ms,
                    max: MAX_COOLDOWN_MS,
                });
            }
            if wake.model.to_str().is_none() {
                return Err(Error::NonUtf8ModelPath {
                    name: wake.name.clone(),
                });
            }
        }
        Ok(())
    }
}

impl FromStr for Config {
    type Err = Error;

    /// Parse + validate a config from inline TOML. Skips `~` expansion
    /// so tests stay deterministic across machines.
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

fn expand_in_place(path: &mut PathBuf) {
    let raw = path.to_string_lossy().into_owned();
    let expanded = shellexpand::tilde(&raw);
    *path = PathBuf::from(expanded.into_owned());
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

    #[test]
    fn rejects_non_16k_sample_rate() {
        let bad = r#"
[engine]
sample_rate = 48000
[engine.shared_models]
melspectrogram = "/m.onnx"
embedding = "/e.onnx"
"#;
        let err = bad.parse::<Config>().unwrap_err();
        assert!(matches!(err, Error::InvalidSampleRate { got: 48000 }));
    }

    #[test]
    fn rejects_threshold_out_of_range() {
        for bad_value in ["1.5", "0.0", "-0.1", "nan"] {
            let bad = format!(
                r#"
[engine]
[engine.shared_models]
melspectrogram = "/m.onnx"
embedding = "/e.onnx"
[[wakeword]]
name = "x"
model = "/x.onnx"
threshold = {bad_value}
"#
            );
            let err = bad.parse::<Config>().unwrap_err();
            assert!(
                matches!(err, Error::InvalidThreshold { .. }),
                "expected InvalidThreshold for {bad_value}, got {err:?}"
            );
        }
    }

    #[test]
    fn rejects_cooldown_above_cap() {
        let bad = r#"
[engine]
[engine.shared_models]
melspectrogram = "/m.onnx"
embedding = "/e.onnx"
[[wakeword]]
name = "x"
model = "/x.onnx"
cooldown_ms = 600000
"#;
        let err = bad.parse::<Config>().unwrap_err();
        assert!(matches!(err, Error::InvalidCooldownMs { got: 600_000, .. }));
    }

    #[test]
    fn accepts_threshold_at_upper_bound() {
        let ok = r#"
[engine]
[engine.shared_models]
melspectrogram = "/m.onnx"
embedding = "/e.onnx"
[[wakeword]]
name = "x"
model = "/x.onnx"
threshold = 1.0
"#;
        let cfg: Config = ok.parse().expect("parse");
        assert!((cfg.wakewords[0].threshold - 1.0).abs() < f64::EPSILON);
    }
}
