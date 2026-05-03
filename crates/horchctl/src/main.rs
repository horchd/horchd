use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use futures_util::StreamExt;
use horchd_client::DaemonProxy;
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(
    name = "horchctl",
    version,
    about = "Control client for the horchd wakeword daemon"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print daemon health (running, audio fps, score fps, mic level, loaded wakewords).
    Status,
    /// Subscribe to the `Detected` signal and print one line per fire. Reconnects on daemon restart.
    Monitor,
    /// Re-read the config file and reconcile in-memory state.
    Reload,

    /// Run all configured wakewords against a WAV file off the live mic
    /// pipeline. Prints one line per detection.
    Process(ProcessArgs),

    /// Manage wakewords (list, add, remove, enable, disable, threshold,
    /// cooldown, import).
    #[command(subcommand)]
    Wakeword(WakewordCommand),

    /// Manage the daemon's audio capture device.
    #[command(subcommand)]
    Device(DeviceCommand),

    /// Inspect the embedded Wyoming-protocol server.
    #[command(subcommand)]
    Wyoming(WyomingCommand),
}

#[derive(Debug, Subcommand)]
enum WyomingCommand {
    /// Show the configured listeners and whether the Wyoming server is enabled.
    Status,
}

#[derive(Debug, Subcommand)]
enum WakewordCommand {
    /// List configured wakewords as a table.
    List,
    /// Register a new wakeword. Validates the model and persists to TOML.
    Add(AddArgs),
    /// Remove a wakeword. The on-disk model file is preserved unless `--purge`.
    Remove {
        name: String,
        #[arg(long)]
        purge: bool,
    },
    /// Enable a wakeword.
    Enable(NameOnly),
    /// Disable a wakeword (the model stays loaded, just stops firing).
    Disable(NameOnly),
    /// Set a wakeword's threshold. Transient by default; pass `--save` to persist to TOML.
    Threshold(ThresholdArgs),
    /// Set a wakeword's cooldown in milliseconds. Use `--save` to persist.
    Cooldown(CooldownArgs),
    /// Import a wakeword model from an HTTP(S) URL or a local path,
    /// stage it under `~/.local/share/horchd/models/`, and register it
    /// with the daemon.
    Import(ImportArgs),
}

#[derive(Debug, Subcommand)]
enum DeviceCommand {
    /// List input devices the daemon's cpal host can see.
    List,
    /// Switch the daemon's audio capture device. `--save` persists to
    /// `[engine].device` in `config.toml`.
    Set(DeviceSetArgs),
}

#[derive(Debug, Args)]
struct ProcessArgs {
    /// Path to a 16 kHz mono int16 WAV file. Use ffmpeg if the format
    /// doesn't match: `ffmpeg -i in.flac -ar 16000 -ac 1 -sample_fmt s16 out.wav`
    file: PathBuf,
    /// Emit one JSON object per line instead of the human-readable table.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct DeviceSetArgs {
    /// cpal device name (use `horchctl device list` to enumerate). Pass
    /// `default` to follow the host's default input.
    name: String,
    /// Persist the change to `[engine].device` in `config.toml`.
    #[arg(long)]
    save: bool,
}

#[derive(Debug, Args)]
struct AddArgs {
    /// ASCII letters / digits / `_` / `-` only.
    name: String,
    #[arg(long)]
    model: PathBuf,
    #[arg(long, default_value_t = horchd_client::Wakeword::DEFAULT_THRESHOLD)]
    threshold: f64,
    #[arg(long, default_value_t = horchd_client::Wakeword::DEFAULT_COOLDOWN_MS)]
    cooldown: u32,
}

#[derive(Debug, Args)]
struct ImportArgs {
    /// Source of the model: an `http(s)://` URL or a local filesystem path.
    source: String,
    /// Register the wakeword under this name. Defaults to the model's
    /// filename stem, sanitized to ASCII letters / digits / `_` / `-`.
    #[arg(long = "as", value_name = "NAME")]
    register_as: Option<String>,
    /// Initial threshold.
    #[arg(long, default_value_t = horchd_client::Wakeword::DEFAULT_THRESHOLD)]
    threshold: f64,
    /// Initial cooldown in milliseconds.
    #[arg(long, default_value_t = horchd_client::Wakeword::DEFAULT_COOLDOWN_MS)]
    cooldown: u32,
    /// Re-download / re-copy + re-register even if the model is
    /// already staged and registered.
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct ThresholdArgs {
    name: String,
    /// New threshold value, range `(0, 1]`.
    value: f64,
    /// Persist the change back to `config.toml` (preserves comments).
    #[arg(long)]
    save: bool,
}

#[derive(Debug, Args)]
struct CooldownArgs {
    name: String,
    /// New cooldown in milliseconds, capped at 60_000.
    value: u32,
    /// Persist the change back to `config.toml` (preserves comments).
    #[arg(long)]
    save: bool,
}

#[derive(Debug, Args)]
struct NameOnly {
    name: String,
    /// Persist the change back to `config.toml`.
    #[arg(long)]
    save: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let conn = zbus::Connection::session()
        .await
        .context("connecting to the D-Bus session bus")?;
    let proxy = DaemonProxy::new(&conn)
        .await
        .context("constructing horchd D-Bus proxy")?;

    match cli.command {
        Command::Status => status(&proxy).await,
        Command::Monitor => monitor(&proxy).await,
        Command::Reload => {
            proxy.reload().await.context("Reload")?;
            println!("reloaded");
            Ok(())
        }
        Command::Process(args) => run_process(&proxy, args).await,
        Command::Wakeword(cmd) => run_wakeword(&proxy, cmd).await,
        Command::Device(cmd) => run_device(&proxy, cmd).await,
        Command::Wyoming(cmd) => run_wyoming(&proxy, cmd).await,
    }
}

async fn run_wyoming(proxy: &DaemonProxy<'_>, cmd: WyomingCommand) -> Result<()> {
    match cmd {
        WyomingCommand::Status => {
            let (enabled, mode, listen) = proxy.wyoming_status().await.context("WyomingStatus")?;
            println!("enabled: {enabled}");
            println!("mode:    {mode}");
            if listen.is_empty() {
                println!("listen:  (none)");
            } else {
                println!("listen:");
                for uri in listen {
                    println!("  {uri}");
                }
            }
            Ok(())
        }
    }
}

async fn run_wakeword(proxy: &DaemonProxy<'_>, cmd: WakewordCommand) -> Result<()> {
    match cmd {
        WakewordCommand::List => list(proxy).await,
        WakewordCommand::Threshold(args) => {
            validate_threshold(args.value)?;
            proxy
                .set_threshold(&args.name, args.value, args.save)
                .await
                .with_context(|| format!("SetThreshold({:?}, {})", args.name, args.value))?;
            println!("threshold of {:?} set to {}", args.name, args.value);
            Ok(())
        }
        WakewordCommand::Cooldown(args) => {
            validate_cooldown(args.value)?;
            proxy
                .set_cooldown(&args.name, args.value, args.save)
                .await
                .with_context(|| format!("SetCooldown({:?}, {})", args.name, args.value))?;
            println!("cooldown of {:?} set to {} ms", args.name, args.value);
            Ok(())
        }
        WakewordCommand::Enable(args) => {
            proxy
                .set_enabled(&args.name, true, args.save)
                .await
                .with_context(|| format!("SetEnabled({:?}, true)", args.name))?;
            println!("{:?} enabled", args.name);
            Ok(())
        }
        WakewordCommand::Disable(args) => {
            proxy
                .set_enabled(&args.name, false, args.save)
                .await
                .with_context(|| format!("SetEnabled({:?}, false)", args.name))?;
            println!("{:?} disabled", args.name);
            Ok(())
        }
        WakewordCommand::Add(args) => {
            validate_name(&args.name)?;
            validate_threshold(args.threshold)?;
            validate_cooldown(args.cooldown)?;
            let model_str = args
                .model
                .to_str()
                .context("model path is not valid UTF-8")?
                .to_owned();
            proxy
                .add(&args.name, &model_str, args.threshold, args.cooldown)
                .await
                .with_context(|| format!("Add({:?}, {model_str:?})", args.name))?;
            println!("added wakeword {:?} (model {})", args.name, model_str);
            Ok(())
        }
        WakewordCommand::Remove { name, purge } => {
            let model_path = if purge {
                proxy
                    .list_wakewords()
                    .await
                    .context("ListWakewords (for --purge)")?
                    .into_iter()
                    .find(|(n, ..)| n == &name)
                    .map(|(_, _, m, _, _)| m)
            } else {
                None
            };
            proxy
                .remove(&name)
                .await
                .with_context(|| format!("Remove({name:?})"))?;
            println!("removed wakeword {name:?}");
            if let Some(path) = model_path {
                purge_model(&path)?;
            }
            Ok(())
        }
        WakewordCommand::Import(args) => run_import(proxy, args).await,
    }
}

async fn run_device(proxy: &DaemonProxy<'_>, cmd: DeviceCommand) -> Result<()> {
    match cmd {
        DeviceCommand::List => {
            let devices = proxy
                .list_input_devices()
                .await
                .context("ListInputDevices")?;
            if devices.is_empty() {
                println!("(no input devices found)");
            } else {
                for d in devices {
                    println!("  {d}");
                }
            }
            Ok(())
        }
        DeviceCommand::Set(args) => {
            proxy
                .set_input_device(&args.name, args.save)
                .await
                .with_context(|| format!("SetInputDevice({:?})", args.name))?;
            println!("input device set to {:?}", args.name);
            Ok(())
        }
    }
}

#[derive(serde::Serialize)]
struct ProcessEntry<'a> {
    timestamp_s: f64,
    name: &'a str,
    score: f64,
}

async fn run_process(proxy: &DaemonProxy<'_>, args: ProcessArgs) -> Result<()> {
    let abs = args
        .file
        .canonicalize()
        .with_context(|| format!("resolving {}", args.file.display()))?;
    let abs_str = abs.to_str().context("path is not valid UTF-8")?;
    let entries = proxy
        .process_audio(abs_str)
        .await
        .with_context(|| format!("ProcessAudio({abs_str:?})"))?;
    if args.json {
        for (name, score, ts_ms) in &entries {
            let entry = ProcessEntry {
                timestamp_s: *ts_ms as f64 / 1000.0,
                name,
                score: *score,
            };
            println!("{}", serde_json::to_string(&entry)?);
        }
    } else if entries.is_empty() {
        eprintln!("no detections");
    } else {
        for (name, score, ts_ms) in &entries {
            println!(
                "{:>9.3}s  {:<20}  score={:.3}",
                *ts_ms as f64 / 1000.0,
                name,
                score
            );
        }
    }
    Ok(())
}

/// Defends against hostile redirects or upstream bugs filling
/// `~/.local/share`. 50 MB is generous — every openWakeWord classifier
/// is ≤ 1 MB.
const MAX_DOWNLOAD_BYTES: u64 = 50 * 1024 * 1024;

async fn run_import(proxy: &DaemonProxy<'_>, args: ImportArgs) -> Result<()> {
    let source = args.source.trim();
    if source.is_empty() {
        bail!("source must be a non-empty URL or path");
    }
    let basename = derive_basename(source)?;
    let register_as = match args.register_as.clone() {
        Some(n) => {
            validate_name(&n)?;
            n
        }
        None => derive_default_name(&basename)?,
    };
    validate_threshold(args.threshold)?;
    validate_cooldown(args.cooldown)?;

    let dest_dir = models_dir()?;
    std::fs::create_dir_all(&dest_dir)
        .with_context(|| format!("creating {}", dest_dir.display()))?;
    let dest = dest_dir.join(&basename);

    if is_url(source) {
        if dest.exists() && !args.force {
            eprintln!(
                "note: {} already exists (use --force to re-download)",
                dest.display()
            );
        } else {
            let digest = download(source, &dest).await?;
            println!("downloaded → {}", dest.display());
            println!("sha256:    {digest}");
        }
    } else {
        stage_local_path(std::path::Path::new(source), &dest, args.force)?;
    }

    let model_str = dest.to_string_lossy().into_owned();
    if args.force {
        // Best-effort cleanup so --force is idempotent. Ignore "not
        // found" — that's the happy path on a fresh install.
        let _ = proxy.remove(&register_as).await;
    }
    proxy
        .add(&register_as, &model_str, args.threshold, args.cooldown)
        .await
        .with_context(|| format!("Add({register_as:?}, {model_str:?})"))?;
    println!(
        "registered wakeword {register_as:?} (model {})",
        dest.display()
    );
    Ok(())
}

fn is_url(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

/// Last URL path segment (before any `?` query) or local-file basename.
fn derive_basename(source: &str) -> Result<String> {
    if is_url(source) {
        let trimmed = source.trim_end_matches('/');
        let last = trimmed.rsplit_once('/').map_or(trimmed, |(_, l)| l);
        let no_query = last.split_once('?').map_or(last, |(p, _)| p);
        if no_query.is_empty() {
            bail!("could not derive a filename from URL {source:?}");
        }
        Ok(no_query.to_string())
    } else {
        std::path::Path::new(source)
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("could not derive a filename from path {source:?}"))
    }
}

/// Sanitize the file stem into a daemon-acceptable wakeword name: keep
/// ASCII letters/digits/`_`/`-`, replace everything else with `_`.
fn derive_default_name(basename: &str) -> Result<String> {
    let stem = std::path::Path::new(basename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(basename);
    let sanitized: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() || sanitized.starts_with('-') {
        bail!("could not derive a wakeword name from {basename:?}; pass `--as <name>`");
    }
    Ok(sanitized)
}

fn stage_local_path(src: &std::path::Path, dest: &std::path::Path, force: bool) -> Result<()> {
    if !src.exists() {
        bail!("source {} does not exist", src.display());
    }
    if !src.is_file() {
        bail!("source {} is not a regular file", src.display());
    }
    let src_canon =
        std::fs::canonicalize(src).with_context(|| format!("canonicalizing {}", src.display()))?;
    let same_as_dest = dest.canonicalize().map(|d| d == src_canon).unwrap_or(false);
    if same_as_dest {
        eprintln!("note: source already lives at {}", dest.display());
        return Ok(());
    }
    if dest.exists() && !force {
        eprintln!(
            "note: {} already exists (use --force to overwrite)",
            dest.display()
        );
        return Ok(());
    }
    std::fs::copy(&src_canon, dest)
        .with_context(|| format!("copying {} → {}", src_canon.display(), dest.display()))?;
    println!("copied → {}", dest.display());
    Ok(())
}

fn models_dir() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME").map_or_else(
        || {
            let home =
                std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("$HOME is not set"))?;
            Ok::<PathBuf, anyhow::Error>(PathBuf::from(home).join(".local").join("share"))
        },
        |v| Ok(PathBuf::from(v)),
    )?;
    Ok(base.join("horchd").join("models"))
}

/// Streaming HTTPS download via reqwest+rustls with a hard size cap and
/// a SHA-256 digest of what was actually written to disk. Returns the
/// hex digest so the caller can echo it for manual verification.
async fn download(url: &str, dest: &std::path::Path) -> Result<String> {
    use std::io::Write as _;

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build()
        .context("building HTTP client")?;
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("HTTP error fetching {url}"))?;

    if let Some(len) = resp.content_length()
        && len > MAX_DOWNLOAD_BYTES
    {
        bail!(
            "{url}: refusing {len}-byte download (cap is {MAX_DOWNLOAD_BYTES}); the URL likely points at the wrong asset",
        );
    }

    let tmp = dest.with_extension("onnx.part");
    let mut file =
        std::fs::File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.with_context(|| format!("reading body chunk from {url}"))?;
        total = total.saturating_add(chunk.len() as u64);
        if total > MAX_DOWNLOAD_BYTES {
            let _ = std::fs::remove_file(&tmp);
            bail!("{url}: download exceeded {MAX_DOWNLOAD_BYTES} bytes; aborting");
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .with_context(|| format!("writing {}", tmp.display()))?;
    }
    file.sync_all().ok();
    drop(file);

    std::fs::rename(&tmp, dest)
        .with_context(|| format!("renaming {} → {}", tmp.display(), dest.display()))?;

    let digest = hasher.finalize();
    Ok(hex(&digest))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}

fn purge_model(model_path: &str) -> Result<()> {
    let path = PathBuf::from(model_path);
    if !path.exists() {
        eprintln!("note: model file {model_path} did not exist on disk");
        return Ok(());
    }
    std::fs::remove_file(&path).with_context(|| format!("deleting model {model_path}"))?;
    println!("purged model file {model_path}");
    let sidecar = path.with_extension("onnx.data");
    if sidecar.exists() {
        std::fs::remove_file(&sidecar)
            .with_context(|| format!("deleting sidecar {}", sidecar.display()))?;
        println!("purged sidecar {}", sidecar.display());
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("wakeword name must not be empty");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        bail!("wakeword name {name:?} must only use ASCII letters, digits, '_' or '-'");
    }
    if name.starts_with('-') {
        bail!("wakeword name must not start with '-'");
    }
    Ok(())
}

fn validate_threshold(value: f64) -> Result<()> {
    if !(value > 0.0 && value <= 1.0) {
        bail!("threshold must be in (0, 1]; got {value}");
    }
    Ok(())
}

fn validate_cooldown(value: u32) -> Result<()> {
    if value > horchd_client::MAX_COOLDOWN_MS {
        bail!(
            "cooldown_ms must be ≤ {} (got {value})",
            horchd_client::MAX_COOLDOWN_MS
        );
    }
    Ok(())
}

async fn status(proxy: &DaemonProxy<'_>) -> Result<()> {
    let (running, audio_fps, score_fps, mic_level) =
        proxy.get_status().await.context("calling GetStatus")?;
    let wakes = proxy
        .list_wakewords()
        .await
        .context("calling ListWakewords")?;

    let state = if running { "running" } else { "stopped" };
    println!("daemon:    {state}");
    println!("audio:     {audio_fps:>6.2} fps");
    println!("score:     {score_fps:>6.2} fps");
    println!("mic level: {mic_level:>6.3}");
    println!("wakewords: {} loaded", wakes.len());
    for (name, threshold, _model, enabled, cooldown_ms) in &wakes {
        let on = if *enabled { "on " } else { "off" };
        println!("  - {on}  {name:<24} threshold {threshold:.3}  cooldown {cooldown_ms} ms");
    }
    Ok(())
}

async fn list(proxy: &DaemonProxy<'_>) -> Result<()> {
    let wakes = proxy
        .list_wakewords()
        .await
        .context("calling ListWakewords")?;
    if wakes.is_empty() {
        println!("(no wakewords configured)");
        return Ok(());
    }

    let name_col = wakes
        .iter()
        .map(|(n, ..)| n.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let model_col = wakes
        .iter()
        .map(|(_, _, m, _, _)| m.len())
        .max()
        .unwrap_or(5)
        .max(5);

    println!(
        "{:<name_col$}  {:<7}  {:<9}  {:<8}  {:<model_col$}",
        "NAME", "ENABLED", "THRESHOLD", "COOLDOWN", "MODEL"
    );
    for (name, threshold, model, enabled, cooldown_ms) in wakes {
        let on = if enabled { "yes" } else { "no" };
        println!(
            "{name:<name_col$}  {on:<7}  {threshold:<9.3}  {ms:<8}  {model:<model_col$}",
            ms = format!("{cooldown_ms} ms")
        );
    }
    Ok(())
}

/// Subscribe to `Detected`. Logs and continues on malformed payloads.
/// Reconnects with exponential backoff (capped) when the daemon goes
/// away — so `horchctl monitor` can sit running while the user
/// `systemctl --user restart horchd`.
async fn monitor(proxy: &DaemonProxy<'_>) -> Result<()> {
    eprintln!("subscribed to xyz.horchd.Daemon1.Detected — press Ctrl-C to exit");
    let mut backoff = Duration::from_millis(500);
    let max_backoff = Duration::from_secs(10);

    loop {
        match monitor_once(proxy).await {
            Ok(()) => {
                eprintln!(
                    "signal stream closed; reconnecting in {:.1}s",
                    backoff.as_secs_f64()
                );
            }
            Err(err) => {
                eprintln!(
                    "monitor error: {err:#}; retrying in {:.1}s",
                    backoff.as_secs_f64()
                );
            }
        }
        tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => {
                eprintln!();
                return Ok(());
            }
            _ = tokio::time::sleep(backoff) => {
                backoff = (backoff * 2).min(max_backoff);
            }
        }
    }
}

async fn monitor_once(proxy: &DaemonProxy<'_>) -> Result<()> {
    let mut stream = proxy
        .receive_detected()
        .await
        .context("subscribing to Detected signal")?;
    loop {
        tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => return Ok(()),
            sig = stream.next() => {
                let Some(sig) = sig else { return Ok(()); };
                match sig.args() {
                    Ok(args) => println!(
                        "{:<10.6}  {:<24}  ts={}",
                        args.score, args.name, args.timestamp_us,
                    ),
                    Err(err) => eprintln!("warning: malformed Detected signal: {err}"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_basename_strips_path_and_query() {
        assert_eq!(
            derive_basename("https://example.com/foo/bar/alexa_v0.1.onnx").unwrap(),
            "alexa_v0.1.onnx"
        );
        assert_eq!(
            derive_basename("http://x.io/m.onnx?token=abc&v=1").unwrap(),
            "m.onnx"
        );
        assert_eq!(
            derive_basename("https://x.io/path/").unwrap_or_default(),
            "path"
        );
    }

    #[test]
    fn path_basename_picks_filename() {
        assert_eq!(derive_basename("/tmp/foo/bar.onnx").unwrap(), "bar.onnx");
        assert_eq!(derive_basename("relative/m.onnx").unwrap(), "m.onnx");
        assert_eq!(
            derive_basename("just-a-file.onnx").unwrap(),
            "just-a-file.onnx"
        );
    }

    #[test]
    fn url_detection_only_accepts_http_schemes() {
        assert!(is_url("http://x"));
        assert!(is_url("https://x"));
        assert!(!is_url("ftp://x"));
        assert!(!is_url("file:///tmp/x.onnx"));
        assert!(!is_url("./local"));
        assert!(!is_url("/abs/path"));
    }

    #[test]
    fn default_name_replaces_invalid_chars() {
        assert_eq!(
            derive_default_name("alexa_v0.1.onnx").unwrap(),
            "alexa_v0_1"
        );
        assert_eq!(
            derive_default_name("hey-jarvis.onnx").unwrap(),
            "hey-jarvis"
        );
        assert_eq!(
            derive_default_name("My Wakeword!.onnx").unwrap(),
            "My_Wakeword_"
        );
    }

    #[test]
    fn default_name_rejects_dash_prefix() {
        assert!(derive_default_name("-leading.onnx").is_err());
    }

    #[test]
    fn default_name_falls_back_to_basename_without_stem() {
        // No extension → use the whole thing
        assert_eq!(derive_default_name("alexa").unwrap(), "alexa");
    }
}
