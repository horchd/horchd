//! WAV file source: streams a `.wav` through the [`AudioSource`] trait.
//!
//! Decode happens on a dedicated blocking task so the inference loop's
//! tokio runtime stays unblocked. Trailing samples that don't fill a
//! complete `FRAME_SAMPLES` window are dropped — the openWakeWord
//! pipeline only operates on full 80 ms windows.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use horchd_client::{
    AudioFrame, AudioSource, FRAME_SAMPLES, SourceDescriptor, SourceKind, TARGET_SAMPLE_RATE,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// 16 frames * 80 ms = 1.28 s of decode-ahead headroom.
const CHANNEL_CAPACITY: usize = 16;

pub struct FileSource {
    path: PathBuf,
    descriptor: SourceDescriptor,
    /// Held so the decoder task drops with the source, stopping mid-stream.
    decoder: Option<JoinHandle<()>>,
}

impl FileSource {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let descriptor = SourceDescriptor {
            name: path.display().to_string(),
            kind: SourceKind::File,
        };
        Self {
            path,
            descriptor,
            decoder: None,
        }
    }
}

impl Drop for FileSource {
    fn drop(&mut self) {
        if let Some(h) = self.decoder.take() {
            h.abort();
        }
    }
}

impl AudioSource for FileSource {
    fn start(&mut self) -> Result<mpsc::Receiver<AudioFrame>> {
        let reader = hound::WavReader::open(&self.path)
            .with_context(|| format!("opening WAV {}", self.path.display()))?;
        validate_spec(reader.spec(), &self.path)?;

        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
        let path = self.path.clone();
        let handle = tokio::task::spawn_blocking(move || stream_samples(reader, tx, &path));
        self.decoder = Some(handle);
        Ok(rx)
    }

    fn descriptor(&self) -> &SourceDescriptor {
        &self.descriptor
    }
}

fn validate_spec(spec: hound::WavSpec, path: &std::path::Path) -> Result<()> {
    let ok = spec.sample_rate == TARGET_SAMPLE_RATE
        && spec.channels == 1
        && spec.sample_format == hound::SampleFormat::Int
        && spec.bits_per_sample == 16;
    if ok {
        return Ok(());
    }
    bail!(
        "{}: unsupported WAV format ({} Hz / {} ch / {:?} {} bit). \
         horchd needs 16000 Hz mono int16. Convert with: \
         ffmpeg -i {} -ar 16000 -ac 1 -sample_fmt s16 fixed.wav",
        path.display(),
        spec.sample_rate,
        spec.channels,
        spec.sample_format,
        spec.bits_per_sample,
        path.display(),
    );
}

fn stream_samples<R: std::io::Read>(
    reader: hound::WavReader<R>,
    tx: mpsc::Sender<AudioFrame>,
    path: &std::path::Path,
) {
    const SCALE: f32 = 1.0 / i16::MAX as f32;
    let mut frame: AudioFrame = Box::new([0.0_f32; FRAME_SAMPLES]);
    let mut pos = 0usize;

    for sample in reader.into_samples::<i16>() {
        let s = match sample {
            Ok(s) => s,
            Err(err) => {
                tracing::error!(?err, path = %path.display(), "WAV decode error");
                return;
            }
        };
        frame[pos] = f32::from(s) * SCALE;
        pos += 1;
        if pos == FRAME_SAMPLES {
            let full = std::mem::replace(&mut frame, Box::new([0.0_f32; FRAME_SAMPLES]));
            if tx.blocking_send(full).is_err() {
                return; // receiver dropped
            }
            pos = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use tempfile::NamedTempFile;

    fn write_test_wav(sample_count: usize, spec: WavSpec) -> NamedTempFile {
        let tmp = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        let mut w = WavWriter::create(tmp.path(), spec).unwrap();
        for i in 0..sample_count {
            // Mild waveform so values are non-trivial.
            let v = (i as f32 / 100.0).sin() * (i16::MAX as f32 * 0.5);
            w.write_sample(v as i16).unwrap();
        }
        w.finalize().unwrap();
        tmp
    }

    fn ok_spec() -> WavSpec {
        WavSpec {
            channels: 1,
            sample_rate: TARGET_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn streams_complete_frames() {
        let tmp = write_test_wav(FRAME_SAMPLES * 3, ok_spec());
        let mut src = FileSource::new(tmp.path());
        let mut rx = src.start().unwrap();
        let mut count = 0;
        while rx.recv().await.is_some() {
            count += 1;
        }
        assert_eq!(count, 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn drops_trailing_partial_frame() {
        let tmp = write_test_wav(FRAME_SAMPLES * 2 + 100, ok_spec());
        let mut src = FileSource::new(tmp.path());
        let mut rx = src.start().unwrap();
        let mut count = 0;
        while rx.recv().await.is_some() {
            count += 1;
        }
        assert_eq!(count, 2, "incomplete trailing window must not be emitted");
    }

    #[test]
    fn rejects_wrong_sample_rate() {
        let tmp = write_test_wav(
            16,
            WavSpec {
                sample_rate: 44_100,
                ..ok_spec()
            },
        );
        let err = FileSource::new(tmp.path()).start().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("16000 Hz"), "msg = {msg}");
        assert!(msg.contains("ffmpeg"), "msg = {msg}");
    }

    #[test]
    fn rejects_stereo() {
        let tmp = write_test_wav(
            16,
            WavSpec {
                channels: 2,
                ..ok_spec()
            },
        );
        let err = FileSource::new(tmp.path()).start().unwrap_err();
        assert!(err.to_string().contains("ffmpeg"));
    }

    #[test]
    fn descriptor_carries_kind_file() {
        let src = FileSource::new("/some/where.wav");
        assert_eq!(src.descriptor().kind, SourceKind::File);
        assert!(src.descriptor().name.contains("where.wav"));
    }
}
