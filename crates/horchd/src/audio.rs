//! Microphone capture: cpal stream → mono 16 kHz frames over a tokio mpsc.
//!
//! Channel layout and rate handling:
//! - Multi-channel input is averaged into mono in the cpal callback.
//! - Sample-rate conversion is integer decimation only (input rate must
//!   be a positive multiple of [`TARGET_SAMPLE_RATE`]). PipeWire typically
//!   honours a 16 kHz request directly; native ALSA at 48 kHz is decimated
//!   3:1. Anything else fails fast — software resampling lands later.
//!
//! Backpressure: the cpal callback runs on a real-time audio thread and
//! must never block. We use [`mpsc::Sender::try_send`]; on overflow the
//! frame is dropped and counted, never blocked.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig, SupportedStreamConfig,
    SupportedStreamConfigRange,
};
use tokio::sync::mpsc;

pub const TARGET_SAMPLE_RATE: u32 = 16_000;
pub const FRAME_SAMPLES: usize = 1280; // 80 ms at 16 kHz

/// One downsampled audio frame ready for the inference pipeline.
pub type Frame = Box<[f32; FRAME_SAMPLES]>;

#[derive(Debug)]
pub struct AudioStats {
    started_at: Instant,
    frames_emitted: AtomicU64,
    frames_dropped: AtomicU64,
    /// Most recent cpal callback's peak |sample|, stored as `f32::to_bits`
    /// in an `AtomicU32` (no atomic floats in std). Range `[0, 1]`.
    last_peak_bits: AtomicU32,
}

impl AudioStats {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            frames_emitted: AtomicU64::new(0),
            frames_dropped: AtomicU64::new(0),
            last_peak_bits: AtomicU32::new(0),
        }
    }

    /// Average frames per second since capture started. Approaches the
    /// nominal 12.5 fps (= 16 kHz / 1280) once the stream has run for a
    /// few seconds.
    pub fn audio_fps(&self) -> f64 {
        let elapsed = self.started_at.elapsed().as_secs_f64();
        match elapsed > 0.0 {
            true => self.frames_emitted.load(Ordering::Relaxed) as f64 / elapsed,
            false => 0.0,
        }
    }

    pub fn frames_emitted(&self) -> u64 {
        self.frames_emitted.load(Ordering::Relaxed)
    }

    pub fn frames_dropped(&self) -> u64 {
        self.frames_dropped.load(Ordering::Relaxed)
    }

    /// Peak `|sample|` of the most recent cpal callback, in `[0, 1]`.
    pub fn last_peak(&self) -> f32 {
        f32::from_bits(self.last_peak_bits.load(Ordering::Relaxed))
    }

    fn record_frame(&self) {
        self.frames_emitted.fetch_add(1, Ordering::Relaxed);
    }

    fn record_drop(&self) {
        self.frames_dropped.fetch_add(1, Ordering::Relaxed);
    }

    fn record_peak(&self, peak: f32) {
        self.last_peak_bits.store(peak.to_bits(), Ordering::Relaxed);
    }
}

/// Owns the live cpal stream + its stats. `Stream` is `!Send` on
/// Linux/ALSA, so this handle must stay on the thread that drives the
/// tokio runtime's `block_on` (the main thread under `#[tokio::main]`).
/// Drop it to stop the audio stream.
pub struct AudioHandle {
    pub stats: Arc<AudioStats>,
    _stream: Stream,
}

/// Open `device_name` (`"default"` for the host default), force mono +
/// 16 kHz via downmix + integer decimation, and start streaming frames.
/// Returns the !Send handle separately from the Send receiver so the
/// receiver can be moved into a tokio task.
pub fn start(
    device_name: &str,
    channel_capacity: usize,
) -> Result<(AudioHandle, mpsc::Receiver<Frame>)> {
    let host = cpal::default_host();
    let device = pick_device(&host, device_name)?;
    let device_label = describe(&device);

    let (chosen, decimation) = select_input_config(&device)
        .with_context(|| format!("negotiating input config for device {device_label:?}"))?;

    let in_rate = chosen.sample_rate();
    let in_channels = chosen.channels();
    let sample_format = chosen.sample_format();
    let stream_cfg = chosen.config();

    let (tx, rx) = mpsc::channel::<Frame>(channel_capacity);
    let stats = Arc::new(AudioStats::new());
    let state = CallbackState::new(in_channels, decimation);

    let stream = build_stream(
        &device,
        &stream_cfg,
        sample_format,
        state,
        tx,
        Arc::clone(&stats),
    )?;
    stream.play().context("starting cpal stream")?;

    tracing::info!(
        device = %device_label,
        in_rate,
        in_channels,
        ?sample_format,
        decimation,
        "audio capture started"
    );

    Ok((
        AudioHandle {
            stats,
            _stream: stream,
        },
        rx,
    ))
}

fn pick_device(host: &cpal::Host, name: &str) -> Result<cpal::Device> {
    if name == "default" {
        return host
            .default_input_device()
            .context("no default input device");
    }
    let mut devices = host.input_devices().context("listing input devices")?;
    devices
        .find(|d| d.description().is_ok_and(|desc| desc.name() == name))
        .with_context(|| format!("input device {name:?} not found"))
}

fn describe(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_owned())
        .unwrap_or_else(|_| "<unknown>".into())
}

/// Negotiate a supported input config that we can stream at 16 kHz mono
/// without software resampling. Strategy:
///
/// 1. Any range that contains 16 kHz exactly → use it (decimation = 1).
/// 2. Otherwise, the smallest max-rate that is an integer multiple of
///    16 kHz (e.g. 32 / 48 / 96 kHz) → decimate down.
/// 3. Otherwise fail — naive integer decimation can't bridge the gap and
///    rubato-style resampling lands in a later phase.
fn select_input_config(device: &cpal::Device) -> Result<(SupportedStreamConfig, usize)> {
    let ranges: Vec<SupportedStreamConfigRange> = device
        .supported_input_configs()
        .context("listing supported input configs")?
        .collect();
    if ranges.is_empty() {
        bail!("device exposes no supported input configurations");
    }

    if let Some(cfg) = pick_exact_target(&ranges) {
        return Ok((cfg, 1));
    }
    if let Some((cfg, decim)) = pick_integer_multiple(&ranges) {
        return Ok((cfg, decim));
    }
    bail!(
        "device offers no 16 kHz / multiple-of-16-kHz input config; ranges: {:?}",
        ranges
            .iter()
            .map(|r| format!(
                "{}ch {}-{}Hz {:?}",
                r.channels(),
                r.min_sample_rate(),
                r.max_sample_rate(),
                r.sample_format(),
            ))
            .collect::<Vec<_>>()
    )
}

fn pick_exact_target(ranges: &[SupportedStreamConfigRange]) -> Option<SupportedStreamConfig> {
    ranges
        .iter()
        .filter(|r| {
            r.min_sample_rate() <= TARGET_SAMPLE_RATE && TARGET_SAMPLE_RATE <= r.max_sample_rate()
        })
        .min_by_key(|r| r.channels())
        .map(|r| r.with_sample_rate(TARGET_SAMPLE_RATE))
}

fn pick_integer_multiple(
    ranges: &[SupportedStreamConfigRange],
) -> Option<(SupportedStreamConfig, usize)> {
    ranges
        .iter()
        .filter(|r| {
            let m = r.max_sample_rate();
            m >= TARGET_SAMPLE_RATE && m.is_multiple_of(TARGET_SAMPLE_RATE)
        })
        .min_by(|a, b| {
            a.max_sample_rate()
                .cmp(&b.max_sample_rate())
                .then_with(|| a.channels().cmp(&b.channels()))
        })
        .map(|r| {
            let cfg = r.with_max_sample_rate();
            let decim = (cfg.sample_rate() / TARGET_SAMPLE_RATE) as usize;
            (cfg, decim)
        })
}

fn build_stream(
    device: &cpal::Device,
    cfg: &StreamConfig,
    fmt: SampleFormat,
    state: CallbackState,
    tx: mpsc::Sender<Frame>,
    stats: Arc<AudioStats>,
) -> Result<Stream> {
    match fmt {
        SampleFormat::F32 => build_typed::<f32>(device, cfg, state, tx, stats),
        SampleFormat::I8 => build_typed::<i8>(device, cfg, state, tx, stats),
        SampleFormat::I16 => build_typed::<i16>(device, cfg, state, tx, stats),
        SampleFormat::I32 => build_typed::<i32>(device, cfg, state, tx, stats),
        SampleFormat::U8 => build_typed::<u8>(device, cfg, state, tx, stats),
        SampleFormat::U16 => build_typed::<u16>(device, cfg, state, tx, stats),
        SampleFormat::U32 => build_typed::<u32>(device, cfg, state, tx, stats),
        other => bail!("unsupported cpal sample format {other:?}"),
    }
    .with_context(|| {
        format!(
            "building input stream at {} Hz, {} ch, {fmt:?}",
            cfg.sample_rate, cfg.channels
        )
    })
}

fn build_typed<S>(
    device: &cpal::Device,
    cfg: &StreamConfig,
    mut state: CallbackState,
    tx: mpsc::Sender<Frame>,
    stats: Arc<AudioStats>,
) -> Result<Stream, cpal::BuildStreamError>
where
    S: SizedSample,
    f32: FromSample<S>,
{
    let err_cb = |err| tracing::error!(?err, "cpal stream error");
    device.build_input_stream(
        cfg,
        move |data: &[S], _info| state.process::<S>(data, &tx, &stats),
        err_cb,
        None,
    )
}

struct CallbackState {
    in_channels: usize,
    decimation: usize,
    decimation_phase: usize,
    frame: Frame,
    frame_pos: usize,
}

impl CallbackState {
    fn new(in_channels: u16, decimation: usize) -> Self {
        Self {
            in_channels: in_channels as usize,
            decimation,
            decimation_phase: 0,
            frame: Box::new([0.0; FRAME_SAMPLES]),
            frame_pos: 0,
        }
    }

    fn process<S>(&mut self, data: &[S], tx: &mpsc::Sender<Frame>, stats: &AudioStats)
    where
        S: SizedSample,
        f32: FromSample<S>,
    {
        let chans = self.in_channels;
        let inv = 1.0 / chans as f32;
        let mut peak = 0.0_f32;
        for chunk in data.chunks_exact(chans) {
            let sum: f32 = chunk.iter().map(|s| f32::from_sample(*s)).sum();
            let mono = sum * inv;
            let abs = mono.abs();
            if abs > peak {
                peak = abs;
            }
            self.feed(mono, tx, stats);
        }
        // EMA-smooth the published peak so the GUI meter doesn't flicker:
        // attack fast, decay slow.
        let prev = stats.last_peak();
        let blended = if peak >= prev {
            peak
        } else {
            prev * 0.78 + peak * 0.22
        };
        stats.record_peak(blended.min(1.0));
    }

    fn feed(&mut self, sample: f32, tx: &mpsc::Sender<Frame>, stats: &AudioStats) {
        let phase = self.decimation_phase;
        self.decimation_phase = (phase + 1) % self.decimation;
        if phase != 0 {
            return;
        }

        self.frame[self.frame_pos] = sample;
        self.frame_pos += 1;
        if self.frame_pos < FRAME_SAMPLES {
            return;
        }

        let full = std::mem::replace(&mut self.frame, Box::new([0.0; FRAME_SAMPLES]));
        self.frame_pos = 0;

        match tx.try_send(full) {
            Ok(()) => stats.record_frame(),
            Err(_) => stats.record_drop(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_range(
        channels: u16,
        min: u32,
        max: u32,
        fmt: SampleFormat,
    ) -> SupportedStreamConfigRange {
        SupportedStreamConfigRange::new(channels, min, max, cpal::SupportedBufferSize::Unknown, fmt)
    }

    #[test]
    fn config_picker_prefers_exact_16khz() {
        let ranges = vec![
            fake_range(2, 44_100, 192_000, SampleFormat::F32),
            fake_range(1, 8_000, 48_000, SampleFormat::F32),
        ];
        let exact = pick_exact_target(&ranges).expect("16k available");
        assert_eq!(exact.sample_rate(), TARGET_SAMPLE_RATE);
        assert_eq!(exact.channels(), 1, "should pick mono over stereo");
    }

    #[test]
    fn config_picker_falls_back_to_integer_multiple() {
        let ranges = vec![
            fake_range(2, 22_050, 96_000, SampleFormat::F32), // 96k = 6 * 16k
            fake_range(1, 22_050, 32_000, SampleFormat::F32), // 32k = 2 * 16k
        ];
        assert!(
            pick_exact_target(&ranges).is_none(),
            "no range covers 16 kHz"
        );
        let (cfg, decim) = pick_integer_multiple(&ranges).expect("integer multiple");
        assert_eq!(cfg.sample_rate(), 32_000, "smallest matching max wins");
        assert_eq!(decim, 2);
    }

    #[test]
    fn config_picker_rejects_pure_44_1khz_device() {
        let ranges = vec![fake_range(2, 44_100, 44_100, SampleFormat::F32)];
        assert!(pick_exact_target(&ranges).is_none());
        assert!(pick_integer_multiple(&ranges).is_none());
    }

    #[test]
    fn callback_emits_frames_at_expected_rate() {
        // 48 kHz mono, decimation 3 → one mono input sample contributes
        // every 3rd raw sample; FRAME_SAMPLES * 3 = 3840 raw samples per frame.
        let mut state = CallbackState::new(1, 3);
        let stats = Arc::new(AudioStats::new());
        let (tx, mut rx) = mpsc::channel::<Frame>(8);
        let raw: Vec<f32> = (0..FRAME_SAMPLES * 3 * 2)
            .map(|i| i as f32 * 1e-4)
            .collect();
        state.process::<f32>(&raw, &tx, &stats);
        drop(tx);

        let mut got = 0;
        while rx.try_recv().is_ok() {
            got += 1;
        }
        assert_eq!(
            got,
            2,
            "expected exactly two frames from {} samples",
            raw.len()
        );
        assert_eq!(stats.frames_emitted(), 2);
        assert_eq!(stats.frames_dropped(), 0);
    }

    #[test]
    fn callback_drops_when_channel_full() {
        let mut state = CallbackState::new(1, 1);
        let stats = Arc::new(AudioStats::new());
        let (tx, _rx) = mpsc::channel::<Frame>(1);
        let raw: Vec<f32> = vec![0.0; FRAME_SAMPLES * 4];
        state.process::<f32>(&raw, &tx, &stats);
        assert_eq!(stats.frames_emitted(), 1);
        assert_eq!(stats.frames_dropped(), 3);
    }

    #[test]
    fn callback_downmixes_stereo() {
        let mut state = CallbackState::new(2, 1);
        let stats = Arc::new(AudioStats::new());
        let (tx, mut rx) = mpsc::channel::<Frame>(2);
        // Stereo interleaved: L=1.0, R=-1.0 → mono mean = 0.0
        let raw: Vec<f32> = (0..FRAME_SAMPLES)
            .flat_map(|_| [1.0_f32, -1.0_f32])
            .collect();
        state.process::<f32>(&raw, &tx, &stats);
        let frame = rx.try_recv().expect("frame");
        assert!(frame.iter().all(|&s| s.abs() < 1e-6));
    }
}
