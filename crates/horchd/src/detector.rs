//! Per-wakeword detection state machine.
//!
//! Fires a [`Detection`] only on a *rising-edge* threshold cross, and
//! only when the cooldown from the previous fire has elapsed. Both
//! rules together prevent a single utterance from emitting back-to-back
//! detections while the score lingers above the threshold.
//!
//! `update` takes an `elapsed: Duration` rather than a wall-clock
//! `Instant` so file replays (where audio runs faster than realtime)
//! get the same cooldown semantics as the live mic. The caller decides
//! the epoch — pipeline-construction for the live mic, file-start for
//! [`crate::pipeline::TransientPipeline`].

use std::time::Duration;

use horchd_client::Detection;

pub struct Detector {
    pub name: String,
    pub threshold: f64,
    pub cooldown: Duration,
    pub enabled: bool,
    was_above: bool,
    last_fire: Option<Duration>,
}

impl Detector {
    pub fn new(name: String, threshold: f64, cooldown_ms: u32, enabled: bool) -> Self {
        Self {
            name,
            threshold,
            cooldown: Duration::from_millis(u64::from(cooldown_ms)),
            enabled,
            was_above: false,
            last_fire: None,
        }
    }

    /// Update with a fresh score and the elapsed time since this
    /// detector's pipeline started. Returns `Some(event)` iff the score
    /// just crossed the threshold from below *and* the cooldown window
    /// has elapsed since the previous fire.
    ///
    /// `Detection.timestamp_us` is set to `elapsed.as_micros()`. The
    /// live mic pipeline overrides this with `CLOCK_MONOTONIC` before
    /// emission for cross-process correlation; the file-replay pipeline
    /// keeps it as the source-relative timestamp.
    ///
    /// `#[must_use]`: the returned event is the only signal that a fire
    /// happened — silently dropping it loses the detection.
    #[must_use]
    pub fn update(&mut self, score: f64, elapsed: Duration) -> Option<Detection> {
        if !self.enabled {
            self.was_above = false;
            return None;
        }
        let above = score >= self.threshold;
        let rising = above && !self.was_above;
        self.was_above = above;
        if !rising {
            return None;
        }
        if let Some(last) = self.last_fire
            && elapsed.saturating_sub(last) < self.cooldown
        {
            return None;
        }
        self.last_fire = Some(elapsed);
        Some(Detection {
            name: self.name.clone(),
            score,
            timestamp_us: u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX),
        })
    }
}

/// `CLOCK_MONOTONIC` microseconds since system boot. The live
/// [`crate::pipeline::Pipeline`] uses this to stamp wire-level
/// `Detected` events so subscribers can correlate with their own
/// `clock_gettime` reads.
pub(crate) fn monotonic_us() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime is signal-safe and only writes through the
    // raw pointer to our local `ts`.
    let ret = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &raw mut ts) };
    if ret != 0 {
        return 0;
    }
    (ts.tv_sec as u64).saturating_mul(1_000_000) + (ts.tv_nsec as u64 / 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make() -> Detector {
        Detector::new("test".into(), 0.5, 1000, true)
    }

    #[test]
    fn fires_on_rising_edge() {
        let mut d = make();
        assert!(
            d.update(0.4, Duration::ZERO).is_none(),
            "below threshold should not fire"
        );
        assert!(
            d.update(0.7, Duration::ZERO).is_some(),
            "rising edge above threshold should fire"
        );
        assert!(
            d.update(0.8, Duration::ZERO).is_none(),
            "still-above is not a rising edge"
        );
        assert!(
            d.update(0.3, Duration::ZERO).is_none(),
            "fall does not fire"
        );
        assert!(
            d.update(0.6, Duration::from_secs(2)).is_some(),
            "second rising edge after cooldown fires"
        );
    }

    #[test]
    fn cooldown_blocks_close_fires() {
        let mut d = make();
        assert!(d.update(0.7, Duration::ZERO).is_some());
        assert!(d.update(0.3, Duration::from_millis(200)).is_none()); // fall
        assert!(
            d.update(0.7, Duration::from_millis(400)).is_none(),
            "rising edge inside cooldown is suppressed"
        );
        assert!(
            d.update(0.7, Duration::from_millis(2000)).is_none(),
            "without an intervening fall the second 0.7 is not a rising edge"
        );
        // Provide the missing fall + rise outside cooldown
        assert!(d.update(0.3, Duration::from_millis(2100)).is_none());
        assert!(
            d.update(0.7, Duration::from_millis(2200)).is_some(),
            "fresh rising edge well past cooldown fires"
        );
    }

    #[test]
    fn disabled_detector_never_fires() {
        let mut d = make();
        d.enabled = false;
        assert!(d.update(0.99, Duration::ZERO).is_none());
    }

    #[test]
    fn threshold_boundary_is_inclusive() {
        let mut d = make();
        assert!(
            d.update(0.5, Duration::ZERO).is_some(),
            "score == threshold counts as above"
        );
    }

    #[test]
    fn detection_carries_elapsed_microseconds() {
        let mut d = make();
        let event = d.update(0.7, Duration::from_millis(2345)).unwrap();
        assert_eq!(event.timestamp_us, 2_345_000);
        assert_eq!(event.name, "test");
        assert!((event.score - 0.7).abs() < 1e-9);
    }
}
