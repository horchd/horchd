/// A wakeword detection emitted by the daemon's detector state machine.
///
/// `timestamp_us` is `CLOCK_MONOTONIC` microseconds since boot, matching
/// the `Detected` D-Bus signal payload.
#[derive(Debug, Clone, PartialEq)]
pub struct WakewordEvent {
    pub name: String,
    pub score: f64,
    pub timestamp_us: u64,
}
