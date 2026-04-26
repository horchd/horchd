use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
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
    }
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
                    Err(err) => eprintln!("malformed Detected signal: {err}"),
                }
            }
        }
    }
}
