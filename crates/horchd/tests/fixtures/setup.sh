#!/usr/bin/env bash
# Fetch the openWakeWord assets needed by the horchd integration tests.
# Run once before `cargo test`; the .onnx + .wav files land next to this
# script and are .gitignored.
#
# License: openWakeWord is Apache-2.0. See ./README.md for attribution.
set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")"

REL="https://github.com/dscripka/openWakeWord/releases/download/v0.5.1"
RAW="https://raw.githubusercontent.com/dscripka/openWakeWord/main"

echo "→ shared preprocessing models (~2.4 MB)"
curl -fsSL -o melspectrogram.onnx  "$REL/melspectrogram.onnx"
curl -fsSL -o embedding_model.onnx "$REL/embedding_model.onnx"

echo "→ alexa classifier (~850 KB)"
curl -fsSL -o alexa_v0.1.onnx "$REL/alexa_v0.1.onnx"

echo "→ alexa test audio (~20 KB)"
curl -fsSL -o alexa_test.wav "$RAW/tests/data/alexa_test.wav"

echo "✓ fixtures ready in $(pwd)"
