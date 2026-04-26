//! `xyz.horchd.Daemon1` D-Bus interface, defined once for all clients.
//!
//! The macro generates [`DaemonProxy`] (async) and [`DaemonProxyBlocking`]
//! for client use. The daemon binary implements the matching server-side
//! `#[zbus::interface]` impl block on its own type and must keep method
//! signatures in sync with this trait.

use zbus::proxy;

/// Wire-level snapshot of one wakeword as returned by
/// [`DaemonProxy::list_wakewords`]: `(name, threshold, model_path,
/// enabled, cooldown_ms)`. Matches D-Bus signature `(sdsbu)`.
pub type WakewordSnapshot = (String, f64, String, bool, u32);

#[proxy(
    interface = "xyz.horchd.Daemon1",
    default_service = "xyz.horchd.Daemon",
    default_path = "/xyz/horchd/Daemon"
)]
pub trait Daemon {
    /// Snapshot of the configured wakewords.
    fn list_wakewords(&self) -> zbus::Result<Vec<WakewordSnapshot>>;

    /// Validate, load and persist a new wakeword. Errors if the model
    /// shape is wrong or if `name` collides with an existing entry.
    fn add(
        &self,
        name: &str,
        model_path: &str,
        threshold: f64,
        cooldown_ms: u32,
    ) -> zbus::Result<()>;

    /// Remove a wakeword from the active set and the config file.
    /// Does **not** delete the on-disk model.
    fn remove(&self, name: &str) -> zbus::Result<()>;

    fn set_threshold(&self, name: &str, threshold: f64, persist: bool) -> zbus::Result<()>;
    fn set_enabled(&self, name: &str, enabled: bool, persist: bool) -> zbus::Result<()>;
    fn set_cooldown(&self, name: &str, ms: u32, persist: bool) -> zbus::Result<()>;

    /// Re-read the config file and reconcile against the live state.
    /// Models that are still configured stay hot; only added / removed /
    /// path-changed entries trigger I/O. The audio thread is preserved.
    fn reload(&self) -> zbus::Result<()>;

    /// `(running, audio_fps, score_fps)`.
    fn get_status(&self) -> zbus::Result<(bool, f64, f64)>;

    /// Emitted on the rising edge when a wakeword's score crosses its
    /// threshold for the first time within a cooldown window.
    /// `timestamp_us` is `CLOCK_MONOTONIC` microseconds since boot.
    #[zbus(signal)]
    fn detected(&self, name: &str, score: f64, timestamp_us: u64) -> zbus::Result<()>;

    /// Low-rate (~5 Hz) per-wakeword score snapshot for live UI meters.
    /// Subscribers can render a continuous score trace without polling
    /// `ListWakewords`. Always-on; subscribe-time decides cost.
    #[zbus(signal)]
    fn score_snapshot(&self, name: &str, score: f64) -> zbus::Result<()>;
}
