//! End-to-end smoke test for the file-driven inference path.
//!
//! Self-skips when the fixtures aren't present — see
//! `crates/horchd/tests/fixtures/setup.sh`.

use std::path::PathBuf;
use std::sync::Arc;

use horchd::audio::FileSource;
use horchd::detector::Detector;
use horchd::inference::{Classifier, InferencePipeline, Preprocessor};
use horchd::pipeline::TransientPipeline;
use horchd::sink::MpscSink;
use horchd_client::{AudioSource as _, DetectionSink};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn alexa_test_wav_fires_through_transient_pipeline() {
    let dir = fixtures_dir();
    let mel = dir.join("melspectrogram.onnx");
    let emb = dir.join("embedding_model.onnx");
    let alexa = dir.join("alexa_v0.1.onnx");
    let wav = dir.join("alexa_test.wav");

    for p in [&mel, &emb, &alexa, &wav] {
        if !p.exists() {
            eprintln!(
                "note: skipping {}; fixture missing: {}\n      run crates/horchd/tests/fixtures/setup.sh",
                module_path!(),
                p.display()
            );
            return;
        }
    }

    let preprocessor = Preprocessor::new(&mel, &emb).expect("preprocessor");
    let classifier = Classifier::load("alexa".to_string(), &alexa).expect("classifier");
    let inference = InferencePipeline::new(preprocessor, vec![classifier]);
    let detectors = vec![Detector::new("alexa".to_string(), 0.5, 1500, true)];

    let (sink, mut rx) = MpscSink::new();
    let sinks: Vec<Arc<dyn DetectionSink>> = vec![Arc::new(sink)];

    let mut source = FileSource::new(&wav);
    let frames = source.start().expect("file source start");

    TransientPipeline::new(inference, detectors, sinks)
        .run(frames)
        .await;
    drop(source);

    let mut hits = Vec::new();
    while let Ok(d) = rx.try_recv() {
        hits.push(d);
    }

    assert!(
        !hits.is_empty(),
        "expected ≥1 alexa detection in alexa_test.wav, got 0"
    );
    let first = &hits[0];
    assert_eq!(first.name, "alexa");
    assert!(
        first.score >= 0.5,
        "first detection score {} is below the threshold",
        first.score
    );
}
