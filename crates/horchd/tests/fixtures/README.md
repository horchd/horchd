# Test fixtures

The integration tests in `crates/horchd/tests/` need a few openWakeWord
assets. They're not committed because they total ~3.3 MB and are
upstream Apache-2.0 artifacts that we shouldn't redistribute by copy.

## One-time setup

```bash
crates/horchd/tests/fixtures/setup.sh
```

Downloads:

| File | Size | Source |
| --- | --- | --- |
| `melspectrogram.onnx` | 1.1 MB | [openWakeWord v0.5.1 release](https://github.com/dscripka/openWakeWord/releases/tag/v0.5.1) |
| `embedding_model.onnx` | 1.3 MB | same |
| `alexa_v0.1.onnx` | 854 KB | same |
| `alexa_test.wav` | 20 KB | `tests/data/` in [openWakeWord main](https://github.com/dscripka/openWakeWord/tree/main/tests/data) |

Tests that depend on these fixtures self-skip with a `note:` if they're
missing — `cargo test` stays green even on a fresh clone.

## License

All four files are © David Scripka and the openWakeWord contributors,
licensed [Apache-2.0](https://github.com/dscripka/openWakeWord/blob/main/LICENSE).
horchd is dual-licensed MIT or Apache-2.0; consuming Apache-2.0 fixtures
under the Apache-2.0 arm is compatible. No NOTICE-file entry is needed
because the assets are not redistributed in the horchd repo.
