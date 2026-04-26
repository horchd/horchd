use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use futures_util::StreamExt;
use horchd_core::DaemonProxy;

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
    /// Print daemon health (running, audio fps, score fps, loaded wakewords).
    Status,
    /// List configured wakewords as a table.
    List,
    /// Subscribe to the `Detected` signal and print one line per fire.
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
    Add {
        name: String,
        #[arg(long)]
        model: PathBuf,
        #[arg(long, default_value_t = 0.5)]
        threshold: f64,
        #[arg(long, default_value_t = 1500)]
        cooldown: u32,
    },
    /// Remove a wakeword. The on-disk model file is preserved unless `--purge`.
    Remove {
        name: String,
        #[arg(long)]
        purge: bool,
    },

    /// Re-read the config file and reconcile in-memory state.
    Reload,
}

#[derive(Debug, Args)]
struct ThresholdArgs {
    /// Wakeword name as defined in the config.
    name: String,
    /// New threshold value (typically in `[0, 1]`).
    value: f64,
    /// Persist the change back to `config.toml` (preserves comments).
    #[arg(long)]
    save: bool,
}

#[derive(Debug, Args)]
struct CooldownArgs {
    /// Wakeword name as defined in the config.
    name: String,
    /// New cooldown in milliseconds.
    value: u32,
    /// Persist the change back to `config.toml` (preserves comments).
    #[arg(long)]
    save: bool,
}

#[derive(Debug, Args)]
struct NameOnly {
    /// Wakeword name as defined in the config.
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
        Command::List => list(&proxy).await,
        Command::Monitor => monitor(&proxy).await,
        Command::Threshold(args) => {
            proxy
                .set_threshold(&args.name, args.value, args.save)
                .await
                .with_context(|| format!("SetThreshold({:?}, {})", args.name, args.value))?;
            println!("threshold of {:?} set to {}", args.name, args.value);
            Ok(())
        }
        Command::Cooldown(args) => {
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
        Command::Add {
            name,
            model,
            threshold,
            cooldown,
        } => {
            let model_str = model
                .to_str()
                .context("model path is not valid UTF-8")?
                .to_owned();
            proxy
                .add(&name, &model_str, threshold, cooldown)
                .await
                .with_context(|| format!("Add({name:?}, {model_str:?})"))?;
            println!("added wakeword {name:?} (model {model_str})");
            Ok(())
        }
        Command::Remove { name, purge } => {
            // Snapshot before we remove so we know the on-disk model path.
            let model_path = if purge {
                proxy.list_wakewords().await.ok().and_then(|wakes| {
                    wakes
                        .into_iter()
                        .find(|(n, ..)| n == &name)
                        .map(|(_, _, m, _, _)| m)
                })
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
    }
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

async fn status(proxy: &DaemonProxy<'_>) -> Result<()> {
    let (running, audio_fps, score_fps) = proxy.get_status().await.context("calling GetStatus")?;
    let wakes = proxy
        .list_wakewords()
        .await
        .context("calling ListWakewords")?;

    let state = if running { "running" } else { "stopped" };
    println!("daemon:    {state}");
    println!("audio:     {audio_fps:>6.2} fps");
    println!("score:     {score_fps:>6.2} fps");
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

    let name_w = wakes
        .iter()
        .map(|(n, ..)| n.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let model_w = wakes
        .iter()
        .map(|(_, _, m, _, _)| m.len())
        .max()
        .unwrap_or(5)
        .max(5);

    println!(
        "{:<name_w$}  {:<7}  {:<9}  {:<8}  {:<model_w$}",
        "NAME", "ENABLED", "THRESHOLD", "COOLDOWN", "MODEL"
    );
    for (name, threshold, model, enabled, cooldown_ms) in wakes {
        let on = if enabled { "yes" } else { "no" };
        println!(
            "{name:<name_w$}  {on:<7}  {threshold:<9.3}  {ms:<8}  {model:<model_w$}",
            ms = format!("{cooldown_ms} ms")
        );
    }
    Ok(())
}

async fn monitor(proxy: &DaemonProxy<'_>) -> Result<()> {
    let mut stream = proxy
        .receive_detected()
        .await
        .context("subscribing to Detected signal")?;
    eprintln!("subscribed to xyz.horchd.Daemon1.Detected — press Ctrl-C to exit");

    loop {
        tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => {
                eprintln!();
                return Ok(());
            }
            sig = stream.next() => {
                let Some(sig) = sig else {
                    eprintln!("signal stream closed");
                    return Ok(());
                };
                match sig.args() {
                    Ok(args) => println!(
                        "{:<10.6}  {:<24}  ts={}",
                        args.score, args.name, args.timestamp_us,
                    ),
                    Err(err) => bail!("malformed Detected signal: {err}"),
                }
            }
        }
    }
}
