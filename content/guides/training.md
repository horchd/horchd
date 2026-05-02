---
eleventyNavigation:
  key: training
  title: Training a wakeword
  parent: guides
  order: 10
description: "How to obtain or train a custom wakeword classifier compatible with horchd's openWakeWord-derived ONNX inference pipeline."
---

`horchd` loads `.onnx` classifiers and runs them. To get a model in the
first place, use one of these paths.

## Importing a model

`horchctl wakeword import` takes either an HTTP(S) URL or a local filesystem
path. It stages the model under `~/.local/share/horchd/models/` and
registers it with the daemon.

### From a URL

```bash
# openWakeWord pretrained models live in the GitHub release:
horchctl wakeword import https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/alexa_v0.1.onnx --as alexa
horchctl wakeword import https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/hey_jarvis_v0.1.onnx --as jarvis --threshold 0.65
```

The default wakeword name is the filename stem with `.` replaced by `_`
(so `alexa_v0.1.onnx` becomes `alexa_v0_1`). Use `--as <name>` to pick
a cleaner name.

### From a local file

```bash
horchctl wakeword import ~/Downloads/my-model.onnx --as my_wake
horchctl wakeword import ./trained/jarvis.onnx --threshold 0.65
```

If the file already lives under `~/.local/share/horchd/models/`, no
copy happens. Otherwise it's copied in.

### Common openWakeWord pretrained models

| File | Suggested `--as` |
| --- | --- |
| `alexa_v0.1.onnx` | `alexa` |
| `hey_jarvis_v0.1.onnx` | `jarvis` |
| `hey_mycroft_v0.1.onnx` | `mycroft` |
| `hey_rhasspy_v0.1.onnx` | `rhasspy` |
| `weather_v0.1.onnx` | `weather` |
| `timer_v0.1.onnx` | `timer` |

All published at
`https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/<file>`.

### Re-importing

`--force` re-downloads / re-copies the model and re-registers it
idempotently (Remove + Add):

```bash
horchctl wakeword import https://example.com/wake.onnx --as wake --force
```

## Train your own with openWakeWord

For a custom wakeword see openWakeWord's
[training docs](https://github.com/dscripka/openWakeWord#training-new-models).
The output is an `.onnx` file with input shape `(N, 16, 96)` and output
`(N, 1)` — exactly what horchd's classifier loader expects. Drop it under
`~/.local/share/horchd/models/` and run `horchctl wakeword add <name> --model …`.

After training, validate the model against a few held-out recordings via
[`horchctl process`](/recipes/process-audio/) before relying on it live.

## In-app training (planned)

The `horchd-gui` Tauri app will eventually ship a Train tab where you
record positive + negative samples through the system mic and produce an
`.onnx` end-to-end. Today that tab is a scaffold — the sample recorder
works, but the training engine itself is not wired up yet. For production
wakewords use one of the two paths above.

## Validation

When you `horchctl wakeword add ...`, the daemon loads the `.onnx` and validates the
shape before accepting it. If you see something like:

```
Error: classifier "jarvis" at /…/jarvis.onnx has shape [N, 12, 96] -> [N, 1],
expected (N, 16, 96) -> (N, 1) — was this model trained for openWakeWord?
```

then the model isn't a 16-frame openWakeWord classifier. Re-export it from
your trainer with the right window size.
