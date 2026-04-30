use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use futures_util::StreamExt;
use horchd_core::DaemonProxy;
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
    /// List configured wakewords as a table.
    List,
    /// Subscribe to the `Detected` signal and print one line per fire. Reconnects on daemon restart.
    Monitor,

    /// Set a wakeword's threshold. Transient by default; pass `--save` to persist to TOML.
    Threshold(ThresholdArgs),
    /// Set a wakeword's cooldown in milliseconds. Use `--save` to persist.
    Cooldown(CooldownArgs),
    /// Enable a wakeword.
    Enable(NameOnly),
    /// Disable a wakeword (the model stays loaded, just stops firing).
    Disable(NameOnly),

    /// Register a new wakeword. Validates the model and persists to TOML.
    Add(AddArgs),
    /// Remove a wakeword. The on-disk model file is preserved unless `--purge`.
    Remove {
        name: String,
        #[arg(long)]
        purge: bool,
    },

    /// Re-read the config file and reconcile in-memory state.
    Reload,

    /// Download an upstream openWakeWord pretrained model into
    /// `~/.local/share/horchd/models/` and register it with the daemon.
    /// Use `--list` to see what's available.
    ImportPretrained(PretrainedArgs),
}

#[derive(Debug, Args)]
struct AddArgs {
    /// ASCII letters / digits / `_` / `-` only.
    name: String,
    #[arg(long)]
    model: PathBuf,
    #[arg(long, default_value_t = horchd_core::Wakeword::DEFAULT_THRESHOLD)]
    threshold: f64,
    #[arg(long, default_value_t = horchd_core::Wakeword::DEFAULT_COOLDOWN_MS)]
    cooldown: u32,
}

#[derive(Debug, Args)]
struct PretrainedArgs {
    /// Pretrained model name (e.g. `hey_jarvis_v0.1`). Omit when using `--list`.
    name: Option<String>,
    /// Print the catalogue of known pretrained models and exit.
    #[arg(long)]
    list: bool,
    /// Register the wakeword under a different name than the model file.
    #[arg(long = "as", value_name = "ALIAS")]
    register_as: Option<String>,
    /// Initial threshold.
    #[arg(long, default_value_t = horchd_core::Wakeword::DEFAULT_THRESHOLD)]
    threshold: f64,
    /// Initial cooldown in milliseconds.
    #[arg(long, default_value_t = horchd_core::Wakeword::DEFAULT_COOLDOWN_MS)]
    cooldown: u32,
    /// Re-download even if the file already exists locally.
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

    // import-pretrained --list and the like don't need the daemon.
    if let Command::ImportPretrained(args) = &cli.command
        && args.list
    {
        print_catalogue();
        return Ok(());
    }

    let conn = zbus::Connection::session()
        .await
        .context("connecting to the D-Bus session bus")?;
    let proxy = DaemonProxy::new(&conn)
        .await
        .context("constructing horchd D-Bus proxy")?;

    match cli.command {
        Command::Status => status(&proxy).await,
        Command::List => list(&proxy).await,
        Command::Monitor => monitor(&proxy).await,
        Command::Threshold(args) => {
            validate_threshold(args.value)?;
            proxy
                .set_threshold(&args.name, args.value, args.save)
                .await
                .with_context(|| format!("SetThreshold({:?}, {})", args.name, args.value))?;
            println!("threshold of {:?} set to {}", args.name, args.value);
            Ok(())
        }
        Command::Cooldown(args) => {
            validate_cooldown(args.value)?;
            proxy
                .set_cooldown(&args.name, args.value, args.save)
                .await
                .with_context(|| format!("SetCooldown({:?}, {})", args.name, args.value))?;
            println!("cooldown of {:?} set to {} ms", args.name, args.value);
            Ok(())
        }
        Command::Enable(args) => {
            proxy
                .set_enabled(&args.name, true, args.save)
                .await
                .with_context(|| format!("SetEnabled({:?}, true)", args.name))?;
            println!("{:?} enabled", args.name);
            Ok(())
        }
        Command::Disable(args) => {
            proxy
                .set_enabled(&args.name, false, args.save)
                .await
                .with_context(|| format!("SetEnabled({:?}, false)", args.name))?;
            println!("{:?} disabled", args.name);
            Ok(())
        }
        Command::Add(args) => {
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
        Command::Remove { name, purge } => {
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
        Command::Reload => {
            proxy.reload().await.context("Reload")?;
            println!("reloaded");
            Ok(())
        }
        Command::ImportPretrained(args) => import_pretrained(&proxy, args).await,
    }
}

const PRETRAINED_BASE: &str = "https://github.com/dscripka/openWakeWord/releases/download/v0.5.1";

const PRETRAINED_CATALOGUE: &[(&str, &str)] = &[
    ("alexa_v0.1", "Alexa"),
    ("hey_jarvis_v0.1", "Hey Jarvis"),
    ("hey_mycroft_v0.1", "Hey Mycroft"),
    ("hey_rhasspy_v0.1", "Hey Rhasspy"),
    ("timer_v0.1", "Timer / set a timer"),
    ("weather_v0.1", "Weather"),
];

/// Maximum bytes we accept from the upstream download — defends against
/// hostile redirects or upstream bugs filling `~/.local/share`.
const MAX_DOWNLOAD_BYTES: u64 = 50 * 1024 * 1024;

fn print_catalogue() {
    println!("Upstream openWakeWord pretrained models");
    println!("---------------------------------------");
    let name_w = PRETRAINED_CATALOGUE
        .iter()
        .map(|(n, _)| n.len())
        .max()
        .unwrap_or(20);
    for (entry, descr) in PRETRAINED_CATALOGUE {
        println!("  {entry:<name_w$}  {descr}");
    }
    println!("\nUsage:  horchctl import-pretrained <name> [--as alias] [--threshold 0.5]");
}

async fn import_pretrained(proxy: &DaemonProxy<'_>, args: PretrainedArgs) -> Result<()> {
    let name = args.name.ok_or_else(|| {
        anyhow::anyhow!("missing model name; try `horchctl import-pretrained --list`")
    })?;
    if !PRETRAINED_CATALOGUE.iter().any(|(n, _)| *n == name) {
        bail!("{name:?} is not in the pretrained catalogue; run `--list` to see options");
    }
    let register_as = args.register_as.clone().unwrap_or_else(|| name.clone());
    validate_name(&register_as)?;
    validate_threshold(args.threshold)?;
    validate_cooldown(args.cooldown)?;

    let dest_dir = models_dir()?;
    std::fs::create_dir_all(&dest_dir)
        .with_context(|| format!("creating {}", dest_dir.display()))?;
    let dest = dest_dir.join(format!("{name}.onnx"));

    if dest.exists() && !args.force {
        eprintln!(
            "note: {} already exists (use --force to re-download)",
            dest.display()
        );
    } else {
        let url = format!("{PRETRAINED_BASE}/{name}.onnx");
        let digest = download(&url, &dest).await?;
        println!("downloaded → {}", dest.display());
        println!("sha256:    {digest}");
    }

    let model_str = dest.to_string_lossy().into_owned();
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
    if value > horchd_core::MAX_COOLDOWN_MS {
        bail!(
            "cooldown_ms must be ≤ {} (got {value})",
            horchd_core::MAX_COOLDOWN_MS
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
