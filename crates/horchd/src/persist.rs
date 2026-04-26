//! Surgical edits to `config.toml` that preserve user comments and
//! formatting via `toml_edit`. Every public function reads the file,
//! mutates the parsed document, and writes it back atomically through
//! a sibling temp file + `rename`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use horchd_core::Wakeword;
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
    let mut tmp: PathBuf = path.to_path_buf();
    let mut name = tmp
        .file_name()
        .map(|n| n.to_owned())
        .unwrap_or_else(|| std::ffi::OsString::from("config"));
    name.push(".horchd-tmp");
    tmp.set_file_name(name);
    fs::write(&tmp, doc.to_string())
        .with_context(|| format!("writing temp file {}", tmp.display()))?;
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

    fn write_tmp(name: &str, body: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("horchd-persist-test-{name}.toml"));
        fs::write(&p, body).unwrap();
        p
    }

    const SAMPLE: &str = r#"# top-of-file comment
[engine]
device = "default"          # mic device
sample_rate = 16000

[engine.shared_models]
melspectrogram = "/m.onnx"
embedding = "/e.onnx"

# Lyna trained this one
[[wakeword]]
name = "lyna"
model = "/lyna.onnx"
threshold = 0.45
cooldown_ms = 1500

[[wakeword]]
name = "jarvis"
model = "/jarvis.onnx"
threshold = 0.7
"#;

    #[test]
    fn set_threshold_preserves_comments_and_formatting() {
        let path = write_tmp("threshold", SAMPLE);
        set_threshold(&path, "lyna", 0.55).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("# top-of-file comment"));
        assert!(after.contains("# Lyna trained this one"));
        assert!(after.contains("# mic device"));
        assert!(after.contains("threshold = 0.55"));
        assert!(after.contains("name = \"jarvis\""));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn add_appends_new_block() {
        let path = write_tmp("add", SAMPLE);
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
        fs::remove_file(&path).ok();
    }

    #[test]
    fn add_rejects_duplicate_name() {
        let path = write_tmp("add-dup", SAMPLE);
        let err = add_wakeword(
            &path,
            &Wakeword {
                name: "lyna".into(),
                model: Path::new("/x.onnx").to_path_buf(),
                threshold: 0.5,
                cooldown_ms: 1500,
                enabled: true,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("already exists"));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn remove_drops_block() {
        let path = write_tmp("remove", SAMPLE);
        remove_wakeword(&path, "jarvis").unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert!(!after.contains("jarvis"));
        assert!(after.contains("lyna"));
        fs::remove_file(&path).ok();
    }
}
