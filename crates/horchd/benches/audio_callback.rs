//! Microbenchmark for the cpal-callback hot path: stereo→mono downmix,
//! decimation, peak-EMA, and channel send. The whole `process` call
//! should be in the low hundreds of ns per input sample at most.
//!
//! Run with `cargo bench -p horchd --bench audio_callback`.

use std::num::NonZeroUsize;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use horchd::audio_for_bench::{AudioFrame, AudioStats, CallbackState, FRAME_SAMPLES};
use tokio::sync::mpsc;

fn drain(rx: &mut mpsc::Receiver<AudioFrame>) {
    while rx.try_recv().is_ok() {}
}

fn bench_callback_mono_no_decim(c: &mut Criterion) {
    let stats = Arc::new(AudioStats::new());
    let (tx, mut rx) = mpsc::channel(16);
    let mut state = CallbackState::new(1, NonZeroUsize::new(1).unwrap());
    let raw: Vec<f32> = (0..FRAME_SAMPLES).map(|i| (i as f32) * 1e-4).collect();
    c.bench_function("callback mono 1280 samples (no decimation)", |b| {
        b.iter(|| {
            state.process::<f32>(&raw, &tx, &stats);
            drain(&mut rx);
        });
    });
}

fn bench_callback_stereo_decim_3(c: &mut Criterion) {
    let stats = Arc::new(AudioStats::new());
    let (tx, mut rx) = mpsc::channel(16);
    let mut state = CallbackState::new(2, NonZeroUsize::new(3).unwrap());
    // 48 kHz stereo → 16 kHz mono: 3840 stereo pairs per emitted frame.
    let raw: Vec<f32> = (0..FRAME_SAMPLES * 3 * 2)
        .map(|i| (i as f32) * 1e-5)
        .collect();
    c.bench_function("callback stereo 3840 samples decimation=3", |b| {
        b.iter(|| {
            state.process::<f32>(&raw, &tx, &stats);
            drain(&mut rx);
        });
    });
}

criterion_group!(
    benches,
    bench_callback_mono_no_decim,
    bench_callback_stereo_decim_3
);
criterion_main!(benches);
