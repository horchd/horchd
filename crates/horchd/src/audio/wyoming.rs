//! Wyoming-streamed audio source.
//!
//! For Mode 2 / Hybrid: a connected Wyoming client pushes `audio-chunk`
//! events containing PCM bytes. The handler decodes those chunks into
//! `Vec<i16>` samples and feeds them through a tokio mpsc into this
//! source, which buffers, normalises to f32, and emits `FRAME_SAMPLES`
//! windows on its own mpsc — exactly what [`crate::pipeline::Pipeline`]
//! expects.
//!
//! v1 only supports the openWakeWord canonical format **16 kHz mono
//! int16**. That's what every shipping HA Wyoming satellite emits
//! (Voice PE, ESP32-S3-BOX, wyoming-satellite, the HA companion app
//! wake-word forwarder). Off-spec formats are rejected at `audio-start`
//! with an actionable error; rubato-based resampling lands in a follow-up
//! once a real-world need surfaces.

use anyhow::{Result, bail};
use horchd_client::{
    AudioFrame, AudioSource, FRAME_SAMPLES, SourceDescriptor, SourceKind, TARGET_SAMPLE_RATE,
};
use horchd_wyoming::audio::AudioStart;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// 16 frames * 80 ms = 1.28 s of buffer headroom against the inference
/// loop momentarily falling behind the client's send rate.
const OUT_CHANNEL_CAPACITY: usize = 16;

/// 256 incoming PCM chunks of headroom — at HA's typical 10 ms cadence
/// that's ~2.5 s of audio buffered before the Wyoming-side mpsc starts
/// blocking the handler. Plenty for the inference loop's 80 ms budget
/// to catch up on a contended core.
const IN_CHANNEL_CAPACITY: usize = 256;

/// Audio sourced from a single Wyoming client connection.
///
/// Construct via [`WyomingSource::new`] which returns the PCM-input
/// [`mpsc::Sender`] alongside the source itself. Drop every clone of
/// the sender to end the source — the consumer pipeline drains the
/// remaining buffered frames and exits naturally.
pub struct WyomingSource {
    descriptor: SourceDescriptor,
    pcm_rx: Option<mpsc::Receiver<Vec<i16>>>,
    /// Held so the framer task drops with the source.
    framer: Option<JoinHandle<()>>,
}

impl WyomingSource {
    /// Returns the PCM sender (handler feeds incoming chunks) and the
    /// source (passed to the inference pipeline). The two halves of the
    /// channel are deliberately separated so dropping the sender is the
    /// signal to end the audio stream.
    pub fn new(peer: impl Into<String>) -> (mpsc::Sender<Vec<i16>>, Self) {
        let (pcm_tx, pcm_rx) = mpsc::channel(IN_CHANNEL_CAPACITY);
        let source = Self {
            descriptor: SourceDescriptor {
                name: peer.into(),
                kind: SourceKind::WyomingStream,
            },
            pcm_rx: Some(pcm_rx),
            framer: None,
        };
        (pcm_tx, source)
    }

    /// Validate the client's `AudioStart` against the openWakeWord
    /// canonical format. Returns a friendly error if anything's off.
    pub fn validate_format(start: &AudioStart) -> Result<()> {
        if start.rate != TARGET_SAMPLE_RATE {
            bail!(
                "Wyoming client offered audio at {} Hz; horchd needs {} Hz \
                 (rubato-based resampling not yet wired)",
                start.rate,
                TARGET_SAMPLE_RATE
            );
        }
        if start.width != 2 {
            bail!(
                "Wyoming client offered {}-byte samples; horchd needs 2-byte int16",
                start.width
            );
        }
        if start.channels != 1 {
            bail!(
                "Wyoming client offered {} channels; horchd needs mono",
                start.channels
            );
        }
        Ok(())
    }
}

impl Drop for WyomingSource {
    fn drop(&mut self) {
        if let Some(h) = self.framer.take() {
            h.abort();
        }
    }
}

impl AudioSource for WyomingSource {
    fn start(&mut self) -> Result<mpsc::Receiver<AudioFrame>> {
        let pcm_rx = self
            .pcm_rx
            .take()
            .ok_or_else(|| anyhow::anyhow!("WyomingSource already started"))?;
        let (frame_tx, frame_rx) = mpsc::channel::<AudioFrame>(OUT_CHANNEL_CAPACITY);
        let handle = tokio::spawn(frame_loop(pcm_rx, frame_tx));
        self.framer = Some(handle);
        Ok(frame_rx)
    }

    fn descriptor(&self) -> &SourceDescriptor {
        &self.descriptor
    }
}

/// Decode incoming `Vec<i16>` chunks into `FRAME_SAMPLES`-sized
/// `f32`-normalised frames. Trailing partial frames at end-of-stream are
/// dropped (same convention as `FileSource`).
async fn frame_loop(mut pcm_rx: mpsc::Receiver<Vec<i16>>, frame_tx: mpsc::Sender<AudioFrame>) {
    const SCALE: f32 = 1.0 / i16::MAX as f32;
    let mut frame: AudioFrame = Box::new([0.0_f32; FRAME_SAMPLES]);
    let mut pos = 0usize;

    while let Some(chunk) = pcm_rx.recv().await {
        for s in chunk {
            frame[pos] = f32::from(s) * SCALE;
            pos += 1;
            if pos == FRAME_SAMPLES {
                let full = std::mem::replace(&mut frame, Box::new([0.0_f32; FRAME_SAMPLES]));
                if frame_tx.send(full).await.is_err() {
                    return;
                }
                pos = 0;
            }
        }
    }
}

/// Decode an `audio-chunk`'s raw payload bytes into `i16` samples.
/// The Wyoming spec says `width` is bytes-per-sample; we only support
/// `width == 2` here (validated upstream by [`WyomingSource::validate_format`]).
pub fn decode_pcm_i16_le(payload: &[u8]) -> Vec<i16> {
    payload
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_start() -> AudioStart {
        AudioStart {
            rate: 16_000,
            width: 2,
            channels: 1,
            timestamp: None,
        }
    }

    #[test]
    fn validate_format_accepts_canonical() {
        WyomingSource::validate_format(&ok_start()).unwrap();
    }

    #[test]
    fn validate_format_rejects_off_rate() {
        let mut s = ok_start();
        s.rate = 48_000;
        let err = WyomingSource::validate_format(&s).unwrap_err();
        assert!(err.to_string().contains("48000"));
    }

    #[test]
    fn validate_format_rejects_stereo() {
        let mut s = ok_start();
        s.channels = 2;
        assert!(WyomingSource::validate_format(&s).is_err());
    }

    #[test]
    fn validate_format_rejects_24bit() {
        let mut s = ok_start();
        s.width = 3;
        assert!(WyomingSource::validate_format(&s).is_err());
    }

    #[test]
    fn decode_pcm_i16_le_round_trips() {
        let bytes = [0x01, 0x00, 0xff, 0x7f, 0x00, 0x80];
        assert_eq!(decode_pcm_i16_le(&bytes), vec![1, i16::MAX, i16::MIN]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn frame_loop_buffers_into_full_frames() {
        let (tx, mut src) = WyomingSource::new("test-peer");
        let mut rx = src.start().unwrap();

        // Irregular chunk sizes summing to exactly FRAME_SAMPLES * 3.
        let chunks = [100usize, 1280, 100, 1080, 1280];
        debug_assert_eq!(chunks.iter().sum::<usize>(), FRAME_SAMPLES * 3);
        for n in chunks {
            tx.send((0..n).map(|i| (i as i16) % 1000).collect())
                .await
                .unwrap();
        }
        drop(tx);

        let mut count = 0;
        while rx.recv().await.is_some() {
            count += 1;
        }
        assert_eq!(count, 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dropping_sender_ends_the_stream() {
        let (tx, mut src) = WyomingSource::new("test-peer");
        let mut rx = src.start().unwrap();
        // Send less than one full frame, then drop sender.
        tx.send(vec![0_i16; 64]).await.unwrap();
        drop(tx);
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn descriptor_carries_kind_wyoming() {
        let (_tx, src) = WyomingSource::new("127.0.0.1:54321");
        assert_eq!(src.descriptor().kind, SourceKind::WyomingStream);
        assert_eq!(src.descriptor().name, "127.0.0.1:54321");
    }
}
