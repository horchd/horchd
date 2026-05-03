//! Surgical edits to `config.toml` that preserve user comments and
//! formatting via `toml_edit`. Every public function reads the file,
//! mutates the parsed document, and writes it back atomically through
//! a sibling temp file + `rename`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use horchd_client::Wakeword;
use toml_edit::{DocumentMut, Item, Table, Value, value as edit_value};

pub fn set_threshold(path: &Path, name: &str, threshold: f64) -> Result<()> {
    update_wakeword(path, name, |table| {
        table["threshold"] = edit_value(threshold);
    })
}

pub fn set_enabled(path: &Path, name: &str, enabled: bool) -> Result<()> {
    update_wakeword(path, name, |table| {
        table["enabled"] = edit_value(enabled);
    })
}

pub fn set_cooldown_ms(path: &Path, name: &str, ms: u32) -> Result<()> {
    update_wakeword(path, name, |table| {
        table["cooldown_ms"] = edit_value(i64::from(ms));
    })
}

pub fn set_engine_device(path: &Path, device: &str) -> Result<()> {
    let mut doc = read_doc(path)?;
    let engine = doc
        .get_mut("engine")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| anyhow::anyhow!("config at {} has no [engine] table", path.display()))?;
    engine["device"] = edit_value(device);
    write_doc(path, &doc)
}

/// Toggle `[wyoming].enabled`, creating the section if it doesn't
/// exist yet (the default config has no `[wyoming]` block at all).
pub fn set_wyoming_enabled(path: &Path, enabled: bool) -> Result<()> {
    let mut doc = read_doc(path)?;
    let needs_init = match doc.get("wyoming") {
        Some(Item::Table(_)) => false,
        Some(_) => bail!(
            "config at {} has a [wyoming] entry that is not a table",
            path.display()
        ),
        None => true,
    };
    if needs_init {
        doc["wyoming"] = Item::Table(Table::new());
    }
    let table = doc["wyoming"].as_table_mut().expect("present after init");
    table["enabled"] = edit_value(enabled);
    write_doc(path, &doc)
}

pub fn add_wakeword(path: &Path, wake: &Wakeword) -> Result<()> {
    let mut doc = read_doc(path)?;
    let arr = wakeword_array_mut(&mut doc, path)?;
    if find_index(arr, &wake.name).is_some() {
        bail!(
            "wakeword {:?} already exists in {}",
            wake.name,
            path.display()
        );
    }
    let mut t = Table::new();
    t["name"] = edit_value(&wake.name);
    t["model"] = edit_value(wake.model.to_string_lossy().as_ref());
    if (wake.threshold - Wakeword::DEFAULT_THRESHOLD).abs() > f64::EPSILON {
        t["threshold"] = edit_value(wake.threshold);
    }
    if wake.cooldown_ms != Wakeword::DEFAULT_COOLDOWN_MS {
        t["cooldown_ms"] = edit_value(i64::from(wake.cooldown_ms));
    }
    if wake.enabled != Wakeword::DEFAULT_ENABLED {
        t["enabled"] = edit_value(wake.enabled);
    }
    arr.push(t);
    write_doc(path, &doc)
}

pub fn remove_wakeword(path: &Path, name: &str) -> Result<()> {
    let mut doc = read_doc(path)?;
    let arr = wakeword_array_mut(&mut doc, path)?;
    let Some(idx) = find_index(arr, name) else {
        bail!("wakeword {name:?} not found in {}", path.display());
    };
    arr.remove(idx);
    write_doc(path, &doc)
}

fn update_wakeword<F: FnOnce(&mut Table)>(path: &Path, name: &str, f: F) -> Result<()> {
    let mut doc = read_doc(path)?;
    let arr = wakeword_array_mut(&mut doc, path)?;
    let Some(idx) = find_index(arr, name) else {
        bail!("wakeword {name:?} not found in {}", path.display());
    };
    let table = arr.get_mut(idx).expect("idx in bounds");
    f(table);
    write_doc(path, &doc)
}

fn wakeword_array_mut<'a>(
    doc: &'a mut DocumentMut,
    path: &Path,
) -> Result<&'a mut toml_edit::ArrayOfTables> {
    let needs_init = match doc.get("wakeword") {
        Some(Item::ArrayOfTables(_)) => false,
        Some(_) => bail!(
            "config at {} has a [wakeword] entry that is not an array of tables",
            path.display()
        ),
        None => true,
    };
    if needs_init {
        doc["wakeword"] = Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
    }
    Ok(doc["wakeword"]
        .as_array_of_tables_mut()
        .expect("present after init"))
}

fn find_index(arr: &toml_edit::ArrayOfTables, name: &str) -> Option<usize> {
    arr.iter().position(|t| {
        t.get("name")
            .and_then(Item::as_value)
            .and_then(Value::as_str)
            == Some(name)
    })
}

fn read_doc(path: &Path) -> Result<DocumentMut> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("reading {} for edit", path.display()))?;
    raw.parse::<DocumentMut>()
        .with_context(|| format!("parsing {} as TOML", path.display()))
}

fn write_doc(path: &Path, doc: &DocumentMut) -> Result<()> {
    use std::io::Write as _;
    let mut tmp: PathBuf = path.to_path_buf();
    let mut name = tmp
        .file_name()
        .map(|n| n.to_owned())
        .unwrap_or_else(|| std::ffi::OsString::from("config"));
    name.push(".horchd-tmp");
    tmp.set_file_name(name);

    // Inherit the original file's permissions so a `chmod 600` on the
    // user's config doesn't get reset to umask defaults on every save.
    let mode = fs::metadata(path).ok().map(|m| m.permissions());

    let mut f =
        fs::File::create(&tmp).with_context(|| format!("creating temp file {}", tmp.display()))?;
    f.write_all(doc.to_string().as_bytes())
        .with_context(|| format!("writing temp file {}", tmp.display()))?;
    // sync_all flushes data + metadata so a crash between write+rename
    // can't leave an empty/torn file behind.
    f.sync_all()
        .with_context(|| format!("fsyncing temp file {}", tmp.display()))?;
    drop(f);
    if let Some(m) = mode
        && let Err(err) = fs::set_permissions(&tmp, m)
    {
        tracing::warn!(?err, path = %tmp.display(), "preserving permissions failed");
    }
    fs::rename(&tmp, path).with_context(|| {
        format!(
            "renaming {} → {} (atomic replace)",
            tmp.display(),
            path.display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn write_tmp(name: &str, body: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(format!("horchd-persist-test-{name}.toml"));
        fs::write(&p, body).unwrap();
        (dir, p)
    }

    const SAMPLE: &str = r#"# top-of-file comment
[engine]
device = "default"          # mic device
sample_rate = 16000

[engine.shared_models]
melspectrogram = "/m.onnx"
embedding = "/e.onnx"

# a custom wakeword
[[wakeword]]
name = "alexa"
model = "/alexa.onnx"
threshold = 0.45
cooldown_ms = 1500

[[wakeword]]
name = "jarvis"
model = "/jarvis.onnx"
threshold = 0.7
"#;

    #[test]
    fn set_threshold_preserves_comments_and_formatting() {
        let (_dir, path) = write_tmp("threshold", SAMPLE);
        set_threshold(&path, "alexa", 0.55).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("# top-of-file comment"));
        assert!(after.contains("# a custom wakeword"));
        assert!(after.contains("# mic device"));
        assert!(after.contains("threshold = 0.55"));
        assert!(after.contains("name = \"jarvis\""));
    }

    #[test]
    fn add_appends_new_block() {
        let (_dir, path) = write_tmp("add", SAMPLE);
        add_wakeword(
            &path,
            &Wakeword {
                name: "wetter".into(),
                model: Path::new("/wetter.onnx").to_path_buf(),
                threshold: 0.6,
                cooldown_ms: 2000,
                enabled: true,
            },
        )
        .unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains(r#"name = "wetter""#));
        assert!(after.contains("threshold = 0.6"));
        assert!(after.contains("cooldown_ms = 2000"));
        // Default `enabled = true` should be omitted to keep TOML tidy
        assert!(
            !after
                .split("[[wakeword]]")
                .last()
                .unwrap()
                .contains("enabled")
        );
    }

    #[test]
    fn add_rejects_duplicate_name() {
        let (_dir, path) = write_tmp("add-dup", SAMPLE);
        let err = add_wakeword(
            &path,
            &Wakeword {
                name: "alexa".into(),
                model: Path::new("/x.onnx").to_path_buf(),
                threshold: 0.5,
                cooldown_ms: 1500,
                enabled: true,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn remove_drops_block() {
        let (_dir, path) = write_tmp("remove", SAMPLE);
        remove_wakeword(&path, "jarvis").unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert!(!after.contains("jarvis"));
        assert!(after.contains("alexa"));
    }

    #[test]
    fn set_engine_device_preserves_top_of_file_comment() {
        let (_dir, path) = write_tmp("engine-device", SAMPLE);
        set_engine_device(&path, "hw:CARD=USB").unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("# top-of-file comment"));
        assert!(after.contains("device = \"hw:CARD=USB\""));
    }

    #[test]
    fn set_enabled_round_trips() {
        let (_dir, path) = write_tmp("enabled", SAMPLE);
        set_enabled(&path, "alexa", false).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("enabled = false"));
        assert!(after.contains("# a custom wakeword"));
    }

    #[test]
    fn set_cooldown_round_trips() {
        let (_dir, path) = write_tmp("cooldown", SAMPLE);
        set_cooldown_ms(&path, "jarvis", 500).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("cooldown_ms = 500"));
    }

    #[test]
    fn add_then_remove_round_trips_to_subset_of_original() {
        let (_dir, path) = write_tmp("add-remove", SAMPLE);
        add_wakeword(
            &path,
            &Wakeword {
                name: "wetter".into(),
                model: Path::new("/wetter.onnx").to_path_buf(),
                threshold: Wakeword::DEFAULT_THRESHOLD,
                cooldown_ms: Wakeword::DEFAULT_COOLDOWN_MS,
                enabled: true,
            },
        )
        .unwrap();
        remove_wakeword(&path, "wetter").unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert!(!after.contains("wetter"));
        assert!(after.contains("alexa"));
        assert!(after.contains("jarvis"));
        // Comments survive both edits
        assert!(after.contains("# top-of-file comment"));
        assert!(after.contains("# a custom wakeword"));
    }
}
