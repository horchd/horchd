//! ONNX inference pipeline: melspectrogram → embedding → per-wakeword
//! classifier.
//!
//! This is a Rust port of openwakeword's `AudioFeatures._streaming_features`
//! (utils.py) plus `Model.predict` (model.py). The shape of every buffer
//! and the order of every operation here matches the Python streaming path
//! so the two implementations can be diffed numerically — see
//! `Preprocessor::feed` for the algorithm walkthrough.
//!
//! Audio scale: cpal hands us f32 in `[-1.0, 1.0]`. The bundled
//! melspectrogram ONNX expects int16-cast-to-f32 (i.e. [-32768, 32767]),
//! so we multiply by `i16::MAX` on the way in.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use ndarray::{Array2, Array3, Array4};
use ort::session::Session;
use ort::value::TensorRef;

use crate::audio::FRAME_SAMPLES;

pub const MEL_BINS: usize = 32;
pub const MEL_HOP_SAMPLES: usize = 160; // 10 ms @ 16 kHz
pub const MEL_OVERLAP_SAMPLES: usize = MEL_HOP_SAMPLES * 3; // 30 ms = 480
pub const RAW_BUFFER_SAMPLES: usize = FRAME_SAMPLES + MEL_OVERLAP_SAMPLES; // 1760

/// Long enough to cover several seconds of mel history (97 frames ≈ 1 s).
/// Matches openwakeword's `melspectrogram_max_len = 10*97`.
pub const MEL_BUFFER_FRAMES: usize = 970;

pub const EMBEDDING_DIM: usize = 96;
pub const EMBEDDING_WINDOW: usize = 76; // mel frames feeding the embedding model

pub const CLASSIFIER_WINDOW: usize = 16; // embedding frames feeding each per-wake model

const MELSPEC_INPUT_NAME: &str = "input";
const EMBEDDING_INPUT_NAME: &str = "input_1";

/// Match openwakeword's "make the ONNX model match the TF Hub model"
/// transform: `spec / 10 + 2`, applied per mel value.
const MELSPEC_TRANSFORM_DIVISOR: f32 = 10.0;
const MELSPEC_TRANSFORM_OFFSET: f32 = 2.0;

/// Streams mono 16 kHz audio frames through melspec + embedding ONNX
/// models and emits one 96-dim embedding per 80 ms input frame.
pub struct Preprocessor {
    melspec: Session,
    embedding: Session,
    raw_buffer: VecDeque<f32>,
    mel_buffer: VecDeque<[f32; MEL_BINS]>,
}

impl Preprocessor {
    pub fn new(melspec_path: &Path, embedding_path: &Path) -> Result<Self> {
        let melspec = Session::builder()
            .context("ort session builder")?
            .commit_from_file(melspec_path)
            .with_context(|| {
                format!(
                    "loading melspectrogram model from {}",
                    melspec_path.display()
                )
            })?;
        let embedding = Session::builder()
            .context("ort session builder")?
            .commit_from_file(embedding_path)
            .with_context(|| {
                format!("loading embedding model from {}", embedding_path.display())
            })?;

        // Warm-start the mel ringbuffer with `1.0` ones, mirroring
        // openwakeword (`melspectrogram_buffer = np.ones((76, 32))`).
        let mut mel_buffer = VecDeque::with_capacity(MEL_BUFFER_FRAMES);
        for _ in 0..EMBEDDING_WINDOW {
            mel_buffer.push_back([1.0_f32; MEL_BINS]);
        }

        Ok(Self {
            melspec,
            embedding,
            raw_buffer: VecDeque::with_capacity(RAW_BUFFER_SAMPLES),
            mel_buffer,
        })
    }

    /// Ingest one 1280-sample frame and emit the resulting 96-dim
    /// embedding. Always returns a value once Phase 4 wiring is live —
    /// the mel ringbuffer is pre-warmed so the embedding model is
    /// always handed 76 frames.
    ///
    /// Algorithm (matches `_streaming_features` in openwakeword/utils.py):
    /// 1. Append the frame (rescaled to int16 range) to the raw buffer,
    ///    keeping only the last `RAW_BUFFER_SAMPLES = 1760` samples.
    /// 2. Run the melspec model over the whole raw buffer (1280 → 5
    ///    frames during the first call, 1760 → 8 frames thereafter).
    ///    Apply the `x/10 + 2` transform.
    /// 3. Append the new mel frames to the mel ringbuffer.
    /// 4. Run the embedding model on the last 76 mel frames; return the
    ///    resulting 96-dim vector.
    pub fn feed(&mut self, frame: &[f32; FRAME_SAMPLES]) -> Result<[f32; EMBEDDING_DIM]> {
        const SCALE: f32 = i16::MAX as f32;
        for &sample in frame.iter() {
            if self.raw_buffer.len() == RAW_BUFFER_SAMPLES {
                self.raw_buffer.pop_front();
            }
            self.raw_buffer.push_back(sample * SCALE);
        }

        let mel_rows = self.run_melspec()?;
        for row in mel_rows {
            if self.mel_buffer.len() == MEL_BUFFER_FRAMES {
                self.mel_buffer.pop_front();
            }
            self.mel_buffer.push_back(row);
        }

        self.run_embedding()
    }

    fn run_melspec(&mut self) -> Result<Vec<[f32; MEL_BINS]>> {
        let len = self.raw_buffer.len();
        let mut samples = Vec::with_capacity(len);
        samples.extend(self.raw_buffer.iter().copied());
        let input = Array2::<f32>::from_shape_vec((1, len), samples)
            .context("building melspec input array")?;

        let outputs = self
            .melspec
            .run(ort::inputs![MELSPEC_INPUT_NAME => TensorRef::from_array_view(&input)?])
            .context("running melspectrogram model")?;
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .context("extracting melspectrogram output")?;

        // openwakeword squeezes the output then walks rows. The melspec
        // ONNX returns shape (1, 1, n_frames, MEL_BINS); squeeze gives
        // (n_frames, MEL_BINS). Expressed as a flat slice this is just
        // `data` of length n_frames * MEL_BINS.
        let total = data.len();
        if !total.is_multiple_of(MEL_BINS) {
            bail!(
                "melspec output length {total} is not a multiple of {MEL_BINS} (shape {:?})",
                shape
            );
        }
        let n_frames = total / MEL_BINS;
        let mut rows = Vec::with_capacity(n_frames);
        for fi in 0..n_frames {
            let mut row = [0.0_f32; MEL_BINS];
            let base = fi * MEL_BINS;
            for (b, slot) in row.iter_mut().enumerate() {
                *slot = data[base + b] / MELSPEC_TRANSFORM_DIVISOR + MELSPEC_TRANSFORM_OFFSET;
            }
            rows.push(row);
        }
        Ok(rows)
    }

    fn run_embedding(&mut self) -> Result<[f32; EMBEDDING_DIM]> {
        let mel_len = self.mel_buffer.len();
        if mel_len < EMBEDDING_WINDOW {
            bail!("mel buffer has {mel_len} frames, need {EMBEDDING_WINDOW} for an embedding");
        }

        let mut input = Array4::<f32>::zeros((1, EMBEDDING_WINDOW, MEL_BINS, 1));
        let start = mel_len - EMBEDDING_WINDOW;
        for (i, row) in self.mel_buffer.iter().skip(start).enumerate() {
            for b in 0..MEL_BINS {
                input[(0, i, b, 0)] = row[b];
            }
        }

        let outputs = self
            .embedding
            .run(ort::inputs![EMBEDDING_INPUT_NAME => TensorRef::from_array_view(&input)?])
            .context("running embedding model")?;
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .context("extracting embedding output")?;
        if data.len() != EMBEDDING_DIM {
            bail!(
                "embedding output has {} values, expected {EMBEDDING_DIM} (shape {:?})",
                data.len(),
                shape
            );
        }
        let mut out = [0.0_f32; EMBEDDING_DIM];
        out.copy_from_slice(data);
        Ok(out)
    }
}

/// Per-wakeword classifier wrapping a Lyna / openwakeword `.onnx` model.
/// Input: `(1, CLASSIFIER_WINDOW=16, EMBEDDING_DIM=96)`. Output: scalar
/// score in `[0, 1]`.
pub struct Classifier {
    pub name: String,
    session: Session,
}

impl Classifier {
    pub fn load(name: String, model_path: &Path) -> Result<Self> {
        let session = Session::builder()
            .context("ort session builder")?
            .commit_from_file(model_path)
            .with_context(|| {
                format!(
                    "loading wakeword model {name:?} from {}",
                    model_path.display()
                )
            })?;
        validate_classifier_shape(&session, &name, model_path)?;
        Ok(Self { name, session })
    }

    /// Score the supplied window. Caller guarantees it is filled in
    /// chronological order with the most recent embedding last.
    pub fn score(&mut self, window: &[[f32; EMBEDDING_DIM]; CLASSIFIER_WINDOW]) -> Result<f32> {
        let mut input = Array3::<f32>::zeros((1, CLASSIFIER_WINDOW, EMBEDDING_DIM));
        for (t, frame) in window.iter().enumerate() {
            for d in 0..EMBEDDING_DIM {
                input[(0, t, d)] = frame[d];
            }
        }
        let outputs = self
            .session
            .run(ort::inputs![TensorRef::from_array_view(&input)?])
            .with_context(|| format!("running classifier {:?}", self.name))?;
        let (_, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .with_context(|| format!("extracting classifier {:?} output", self.name))?;
        data.first()
            .copied()
            .with_context(|| format!("classifier {:?} returned empty output", self.name))
    }
}

fn validate_classifier_shape(session: &Session, name: &str, path: &Path) -> Result<()> {
    let Some(in_outlet) = session.inputs().first() else {
        bail!(
            "classifier {name:?} at {} exposes no inputs",
            path.display()
        );
    };
    let Some(out_outlet) = session.outputs().first() else {
        bail!(
            "classifier {name:?} at {} exposes no outputs",
            path.display()
        );
    };

    let in_dims: Vec<i64> = in_outlet
        .dtype()
        .tensor_shape()
        .map(|s| s.iter().copied().collect())
        .unwrap_or_default();
    let out_dims: Vec<i64> = out_outlet
        .dtype()
        .tensor_shape()
        .map(|s| s.iter().copied().collect())
        .unwrap_or_default();

    let in_ok = in_dims.len() == 3
        && dim_matches(in_dims[1], CLASSIFIER_WINDOW as i64)
        && dim_matches(in_dims[2], EMBEDDING_DIM as i64);
    let out_ok = out_dims.len() == 2 && dim_matches(out_dims[1], 1);
    if in_ok && out_ok {
        return Ok(());
    }

    bail!(
        "classifier {name:?} at {} has shape {in_dims:?} -> {out_dims:?}, expected (N, {CLASSIFIER_WINDOW}, {EMBEDDING_DIM}) -> (N, 1) — was this model trained for openWakeWord?",
        path.display(),
    )
}

/// `<= 0` means the dim is symbolic / dynamic (e.g. variable batch size);
/// anything positive must equal `expected`.
fn dim_matches(dim: i64, expected: i64) -> bool {
    dim <= 0 || dim == expected
}

/// Counts inference work for the `score_fps` field of `GetStatus`.
/// At steady state matches the audio fps (12.5).
#[derive(Debug)]
pub struct InferenceStats {
    started_at: Instant,
    scores_emitted: AtomicU64,
}

impl InferenceStats {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            scores_emitted: AtomicU64::new(0),
        }
    }

    pub fn score_fps(&self) -> f64 {
        let elapsed = self.started_at.elapsed().as_secs_f64();
        match elapsed > 0.0 {
            true => self.scores_emitted.load(Ordering::Relaxed) as f64 / elapsed,
            false => 0.0,
        }
    }

    pub fn scores_emitted(&self) -> u64 {
        self.scores_emitted.load(Ordering::Relaxed)
    }

    pub fn record_score(&self) {
        self.scores_emitted.fetch_add(1, Ordering::Relaxed);
    }
}

impl Default for InferenceStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Combines a [`Preprocessor`] with N [`Classifier`]s and a 16-frame
/// embedding window. Call [`InferencePipeline::process`] for every
/// audio frame; it returns one score per classifier.
pub struct InferencePipeline {
    preprocessor: Preprocessor,
    classifiers: Vec<Classifier>,
    window: VecDeque<[f32; EMBEDDING_DIM]>,
}

impl InferencePipeline {
    pub fn new(preprocessor: Preprocessor, classifiers: Vec<Classifier>) -> Self {
        let mut window = VecDeque::with_capacity(CLASSIFIER_WINDOW);
        for _ in 0..CLASSIFIER_WINDOW {
            window.push_back([0.0_f32; EMBEDDING_DIM]);
        }
        Self {
            preprocessor,
            classifiers,
            window,
        }
    }

    /// Returns one `(name, score)` per classifier, in the order they
    /// were loaded.
    pub fn process(&mut self, frame: &[f32; FRAME_SAMPLES]) -> Result<Vec<(String, f32)>> {
        let embedding = self.preprocessor.feed(frame)?;
        if self.window.len() == CLASSIFIER_WINDOW {
            self.window.pop_front();
        }
        self.window.push_back(embedding);

        let mut buf = [[0.0_f32; EMBEDDING_DIM]; CLASSIFIER_WINDOW];
        for (i, frame) in self.window.iter().enumerate() {
            buf[i] = *frame;
        }

        let mut scores = Vec::with_capacity(self.classifiers.len());
        for clf in &mut self.classifiers {
            let score = clf.score(&buf)?;
            scores.push((clf.name.clone(), score));
        }
        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_self_consistent() {
        const { assert!(MEL_HOP_SAMPLES * 3 == MEL_OVERLAP_SAMPLES) };
        const { assert!(FRAME_SAMPLES + MEL_OVERLAP_SAMPLES == RAW_BUFFER_SAMPLES) };
        const { assert!(EMBEDDING_WINDOW == 76) };
        const { assert!(EMBEDDING_WINDOW <= MEL_BUFFER_FRAMES) };
    }
}
