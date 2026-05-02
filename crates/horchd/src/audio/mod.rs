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

pub mod mic;

pub use mic::MicSource;

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig, SupportedStreamConfig,
    SupportedStreamConfigRange,
};
use horchd_client::{AudioFrame, FRAME_SAMPLES, TARGET_SAMPLE_RATE};
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct AudioStats {
    started_at: Instant,
    frames_emitted: AtomicU64,
    frames_dropped: AtomicU64,
    /// Most recent cpal callback's peak |sample|, stored as `f32::to_bits`
    /// in an `AtomicU32` (no atomic floats in std). Range `[0, 1]`.
    last_peak_bits: AtomicU32,
}

impl Default for AudioStats {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioStats {
    pub fn new() -> Self {
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

/// Enumerate input devices that expose at least one usable config.
/// Filtering by config presence drops cpal/ALSA stubs (output-only
/// entries that show up in the raw `input_devices()` iterator on Linux
/// but aren't real captures).
pub fn list_input_device_names() -> Result<Vec<String>> {
    let host = cpal::default_host();
    let mut names: Vec<String> = host
        .input_devices()
        .context("listing input devices")?
        .filter(|d| {
            d.supported_input_configs()
                .map(|mut it| it.next().is_some())
                .unwrap_or(false)
        })
        .filter_map(|d| d.description().ok().map(|desc| desc.name().to_owned()))
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

/// Open `device_name` (`"default"` for the host default), force mono +
/// 16 kHz via downmix + integer decimation, and start streaming frames.
/// Returns the `!Send` cpal stream (drop to stop) along with the
/// resolved device label and the Send-able frame receiver.
pub(crate) fn open_input_stream(
    device_name: &str,
    channel_capacity: usize,
    stats: Arc<AudioStats>,
) -> Result<(Stream, String, mpsc::Receiver<AudioFrame>)> {
    let host = cpal::default_host();
    let device = pick_device(&host, device_name)?;
    let device_label = describe(&device);

    let (chosen, decimation) = select_input_config(&device)
        .with_context(|| format!("negotiating input config for device {device_label:?}"))?;

    let in_rate = chosen.sample_rate();
    let in_channels = chosen.channels();
    let sample_format = chosen.sample_format();
    let stream_cfg = chosen.config();

    let (tx, rx) = mpsc::channel::<AudioFrame>(channel_capacity);
    let state = CallbackState::new(in_channels, decimation);

    let stream = build_stream(&device, &stream_cfg, sample_format, state, tx, stats)?;
    stream.play().context("starting cpal stream")?;

    tracing::info!(
        device = %device_label,
        in_rate,
        in_channels,
        ?sample_format,
        decimation,
        "audio capture started"
    );

    Ok((stream, device_label, rx))
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
fn select_input_config(device: &cpal::Device) -> Result<(SupportedStreamConfig, NonZeroUsize)> {
    let ranges: Vec<SupportedStreamConfigRange> = device
        .supported_input_configs()
        .context("listing supported input configs")?
        .collect();
    if ranges.is_empty() {
        bail!("device exposes no supported input configurations");
    }

    if let Some(cfg) = pick_exact_target(&ranges) {
        return Ok((cfg, NonZeroUsize::new(1).expect("1 != 0")));
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

/// Lower = better. Wide-precision floats first, then ints, then 8-bit.
/// 8-bit captures sound terrible (≤ 256 levels per sample = visibly
/// quiet meter and audibly grainy preprocessor input).
fn sample_format_pref(fmt: SampleFormat) -> u8 {
    match fmt {
        SampleFormat::F32 | SampleFormat::F64 => 0,
        SampleFormat::I32 | SampleFormat::U32 => 1,
        SampleFormat::I16 | SampleFormat::U16 => 2,
        SampleFormat::I8 | SampleFormat::U8 => 3,
        _ => 4,
    }
}

fn pick_exact_target(ranges: &[SupportedStreamConfigRange]) -> Option<SupportedStreamConfig> {
    ranges
        .iter()
        .filter(|r| {
            r.min_sample_rate() <= TARGET_SAMPLE_RATE && TARGET_SAMPLE_RATE <= r.max_sample_rate()
        })
        .min_by(|a, b| {
            sample_format_pref(a.sample_format())
                .cmp(&sample_format_pref(b.sample_format()))
                .then_with(|| a.channels().cmp(&b.channels()))
        })
        .map(|r| r.with_sample_rate(TARGET_SAMPLE_RATE))
}

fn pick_integer_multiple(
    ranges: &[SupportedStreamConfigRange],
) -> Option<(SupportedStreamConfig, NonZeroUsize)> {
    ranges
        .iter()
        .filter(|r| {
            let m = r.max_sample_rate();
            m > TARGET_SAMPLE_RATE && m.is_multiple_of(TARGET_SAMPLE_RATE)
        })
        .min_by(|a, b| {
            sample_format_pref(a.sample_format())
                .cmp(&sample_format_pref(b.sample_format()))
                .then_with(|| a.max_sample_rate().cmp(&b.max_sample_rate()))
                .then_with(|| a.channels().cmp(&b.channels()))
        })
        .and_then(|r| {
            let cfg = r.with_max_sample_rate();
            let raw = (cfg.sample_rate() / TARGET_SAMPLE_RATE) as usize;
            NonZeroUsize::new(raw).map(|d| (cfg, d))
        })
}

fn build_stream(
    device: &cpal::Device,
    cfg: &StreamConfig,
    fmt: SampleFormat,
    state: CallbackState,
    tx: mpsc::Sender<AudioFrame>,
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
    tx: mpsc::Sender<AudioFrame>,
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

pub struct CallbackState {
    in_channels: usize,
    /// Encoded as `NonZeroUsize` so the `phase % decimation` in `feed`
    /// can never panic, even under future refactors.
    decimation: NonZeroUsize,
    decimation_phase: usize,
    frame: AudioFrame,
    frame_pos: usize,
    /// Pre-allocated swap buffer so the cpal callback (real-time audio
    /// thread) doesn't allocate when emitting a frame; we just `swap`
    /// the two boxes when one is full.
    spare: AudioFrame,
}

impl CallbackState {
    pub fn new(in_channels: u16, decimation: NonZeroUsize) -> Self {
        Self {
            in_channels: in_channels as usize,
            decimation,
            decimation_phase: 0,
            frame: Box::new([0.0; FRAME_SAMPLES]),
            frame_pos: 0,
            spare: Box::new([0.0; FRAME_SAMPLES]),
        }
    }

    /// Hot-path: convert one cpal callback's interleaved buffer into 16
    /// kHz mono frames. `pub` so benches and integration tests can
    /// drive it without spinning up cpal.
    pub fn process<S>(&mut self, data: &[S], tx: &mpsc::Sender<AudioFrame>, stats: &AudioStats)
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

    fn feed(&mut self, sample: f32, tx: &mpsc::Sender<AudioFrame>, stats: &AudioStats) {
        let phase = self.decimation_phase;
        self.decimation_phase = (phase + 1) % self.decimation.get();
        if phase != 0 {
            return;
        }

        self.frame[self.frame_pos] = sample;
        self.frame_pos += 1;
        if self.frame_pos < FRAME_SAMPLES {
            return;
        }

        // Swap the full buffer out for the pre-allocated spare — no
        // allocator call on the realtime audio thread. If the consumer
        // can't keep up, we drop the frame and the still-full box rides
        // along as the next spare.
        std::mem::swap(&mut self.frame, &mut self.spare);
        self.frame_pos = 0;
        match tx.try_send(std::mem::replace(
            &mut self.spare,
            Box::new([0.0; FRAME_SAMPLES]),
        )) {
            Ok(()) => stats.record_frame(),
            Err(mpsc::error::TrySendError::Full(returned)) => {
                // Reuse the rejected box — it stays as our spare so the
                // next frame doesn't alloc either.
                self.spare = returned;
                stats.record_drop();
            }
            Err(mpsc::error::TrySendError::Closed(_)) => stats.record_drop(),
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
        assert_eq!(decim.get(), 2);
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
        let mut state = CallbackState::new(1, NonZeroUsize::new(3).unwrap());
        let stats = Arc::new(AudioStats::new());
        let (tx, mut rx) = mpsc::channel::<AudioFrame>(8);
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
        let mut state = CallbackState::new(1, NonZeroUsize::new(1).unwrap());
        let stats = Arc::new(AudioStats::new());
        let (tx, _rx) = mpsc::channel::<AudioFrame>(1);
        let raw: Vec<f32> = vec![0.0; FRAME_SAMPLES * 4];
        state.process::<f32>(&raw, &tx, &stats);
        assert_eq!(stats.frames_emitted(), 1);
        assert_eq!(stats.frames_dropped(), 3);
    }

    #[test]
    fn callback_downmixes_stereo() {
        let mut state = CallbackState::new(2, NonZeroUsize::new(1).unwrap());
        let stats = Arc::new(AudioStats::new());
        let (tx, mut rx) = mpsc::channel::<AudioFrame>(2);
        // Stereo interleaved: L=1.0, R=-1.0 → mono mean = 0.0
        let raw: Vec<f32> = (0..FRAME_SAMPLES)
            .flat_map(|_| [1.0_f32, -1.0_f32])
            .collect();
        state.process::<f32>(&raw, &tx, &stats);
        let frame = rx.try_recv().expect("frame");
        assert!(frame.iter().all(|&s| s.abs() < 1e-6));
    }
}
