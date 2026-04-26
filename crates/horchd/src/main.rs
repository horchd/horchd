// horchd — native multi-wakeword detection daemon.
// Currently a placeholder; see plan.md at repo root for the build roadmap.

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("horchd 0.1.0 — not implemented yet, see plan.md");
    Ok(())
}
