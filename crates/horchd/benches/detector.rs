//! Microbenchmark for the per-wakeword threshold + cooldown state
//! machine. Sanity check: `update` is < 100 ns even at 1 M iterations
//! so it's effectively free in the audio loop.
//!
//! Run with `cargo bench -p horchd --bench detector`.

use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use horchd::detector_for_bench::Detector;

fn bench_detector_update(c: &mut Criterion) {
    let mut det = Detector::new("alexa".into(), 0.5, 1500, true);
    let mut elapsed = Duration::ZERO;
    let mut score = 0.0_f64;
    c.bench_function("Detector::update steady-state", |b| {
        b.iter(|| {
            score = if score > 0.5 { 0.3 } else { 0.7 };
            elapsed += Duration::from_millis(80);
            std::hint::black_box(det.update(score, elapsed));
        });
    });
}

fn bench_detector_update_disabled(c: &mut Criterion) {
    let mut det = Detector::new("alexa".into(), 0.5, 1500, false);
    c.bench_function("Detector::update disabled", |b| {
        b.iter(|| {
            std::hint::black_box(det.update(0.7, Duration::ZERO));
        });
    });
}

criterion_group!(
    benches,
    bench_detector_update,
    bench_detector_update_disabled
);
criterion_main!(benches);
