//! Per-wakeword detection state machine.
//!
//! Fires a [`WakewordEvent`] only on a *rising-edge* threshold cross,
//! and only when the cooldown from the previous fire has elapsed. Both
//! rules together prevent a single utterance from emitting back-to-back
//! detections while the score lingers above the threshold.

use std::time::{Duration, Instant};

use horchd_core::WakewordEvent;

pub struct Detector {
    pub name: String,
    pub threshold: f64,
    pub cooldown: Duration,
    pub enabled: bool,
    was_above: bool,
    last_fire: Option<Instant>,
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

    /// Update with a fresh score and `now`. Returns `Some(event)` iff
    /// the score just crossed the threshold from below *and* the
    /// cooldown window has elapsed since the previous fire.
    ///
    /// `#[must_use]`: the returned event is the only signal that a fire
    /// happened — silently dropping it loses the detection.
    #[must_use]
    pub fn update(&mut self, score: f64, now: Instant) -> Option<WakewordEvent> {
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
            && now.duration_since(last) < self.cooldown
        {
            return None;
        }
        self.last_fire = Some(now);
        Some(WakewordEvent {
            name: self.name.clone(),
            score,
            timestamp_us: monotonic_us(),
        })
    }
}

/// `CLOCK_MONOTONIC` microseconds since system boot. Matches the
/// wire format the `Detected` D-Bus signal advertises.
fn monotonic_us() -> u64 {
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

    fn make() -> (Detector, Instant) {
        (
            Detector::new("test".into(), 0.5, 1000, true),
            Instant::now(),
        )
    }

    #[test]
    fn fires_on_rising_edge() {
        let (mut d, t0) = make();
        assert!(
            d.update(0.4, t0).is_none(),
            "below threshold should not fire"
        );
        assert!(
            d.update(0.7, t0).is_some(),
            "rising edge above threshold should fire"
        );
        assert!(
            d.update(0.8, t0).is_none(),
            "still-above is not a rising edge"
        );
        assert!(d.update(0.3, t0).is_none(), "fall does not fire");
        assert!(
            d.update(0.6, t0 + Duration::from_secs(2)).is_some(),
            "second rising edge after cooldown fires"
        );
    }

    #[test]
    fn cooldown_blocks_close_fires() {
        let (mut d, t0) = make();
        assert!(d.update(0.7, t0).is_some());
        assert!(d.update(0.3, t0 + Duration::from_millis(200)).is_none()); // fall
        assert!(
            d.update(0.7, t0 + Duration::from_millis(400)).is_none(),
            "rising edge inside cooldown is suppressed"
        );
        assert!(
            d.update(0.7, t0 + Duration::from_millis(2000)).is_none(),
            "without an intervening fall the second 0.7 is not a rising edge"
        );
        // Provide the missing fall + rise outside cooldown
        assert!(d.update(0.3, t0 + Duration::from_millis(2100)).is_none());
        assert!(
            d.update(0.7, t0 + Duration::from_millis(2200)).is_some(),
            "fresh rising edge well past cooldown fires"
        );
    }

    #[test]
    fn disabled_detector_never_fires() {
        let (mut d, t0) = make();
        d.enabled = false;
        assert!(d.update(0.99, t0).is_none());
    }

    #[test]
    fn threshold_boundary_is_inclusive() {
        let (mut d, t0) = make();
        assert!(
            d.update(0.5, t0).is_some(),
            "score == threshold counts as above"
        );
    }
}
