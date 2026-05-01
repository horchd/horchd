---
eleventyNavigation:
  key: rust-subscriber
  title: Rust
  parent: recipes
  order: 30
description: "Subscribe to xyz.horchd.Daemon1.Detected from Rust using zbus and the published horchd-client proxy trait."
---

The proxy trait that `horchctl` uses is published as part of the
[`horchd-client`](https://codeberg.org/NewtTheWolf/horchd/src/branch/main/crates/horchd-client)
library crate. Add it as a path or git dependency:

```toml
[dependencies]
horchd-client  = { git = "https://codeberg.org/NewtTheWolf/horchd" }
zbus           = "5"
tokio          = { version = "1", features = ["macros", "rt-multi-thread", "signal"] }
futures-util   = "0.3"
```

```rust
use anyhow::Result;
use futures_util::StreamExt;
use horchd_client::DaemonProxy;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let conn = zbus::Connection::session().await?;
    let proxy = DaemonProxy::new(&conn).await?;

    // Snapshot
    let wakes = proxy.list_wakewords().await?;
    for (name, threshold, model, enabled, cooldown_ms) in &wakes {
        println!(
            "{name:<20} {:<5} threshold={threshold:.3} cooldown={cooldown_ms}ms model={model}",
            if *enabled { "on" } else { "off" }
        );
    }

    // Subscribe
    let mut stream = proxy.receive_detected().await?;
    while let Some(sig) = stream.next().await {
        let args = sig.args()?;
        println!(
            "{}\tscore={:.4}\tts={}",
            args.name, args.score, args.timestamp_us
        );
    }
    Ok(())
}
```

## Without `horchd-client`

If you'd rather not pull in the path dep, declare the proxy trait yourself
with `#[zbus::proxy]`:

```rust
use zbus::proxy;

#[proxy(
    interface = "xyz.horchd.Daemon1",
    default_service = "xyz.horchd.Daemon",
    default_path = "/xyz/horchd/Daemon"
)]
trait Daemon {
    fn list_wakewords(&self) -> zbus::Result<Vec<(String, f64, String, bool, u32)>>;
    fn get_status(&self) -> zbus::Result<(bool, f64, f64, f64)>;

    #[zbus(signal)]
    fn detected(&self, name: &str, score: f64, timestamp_us: u64) -> zbus::Result<()>;
}
```

That's the entire surface area you need to receive `Detected` signals — the
crate is mostly there to keep the type definitions DRY between the daemon,
`horchctl`, and your subscriber.
