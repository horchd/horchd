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

## Pretrained openWakeWord models

[`openwakeword`](https://github.com/dscripka/openWakeWord) (the Python
package horchd ports the inference path from) ships a set of pretrained
models. The fastest way to get a working wakeword is to import one of them
via `horchctl`:

```bash
horchctl import-pretrained --list
horchctl import-pretrained hey_jarvis_v0.1
horchctl import-pretrained hey_jarvis_v0.1 --as jarvis --threshold 0.65
```

This downloads the `.onnx` into `~/.local/share/horchd/models/` and
registers it. You can also do it by hand from a local Python install:

```bash
oww=$(python -c 'import openwakeword, pathlib; print(pathlib.Path(openwakeword.__file__).parent / "resources/models")')
ls "$oww"/*.onnx
cp "$oww/hey_jarvis_v0.1.onnx" ~/.local/share/horchd/models/
horchctl add hey_jarvis --model ~/.local/share/horchd/models/hey_jarvis_v0.1.onnx
```

You'll find `alexa_v0.1.onnx`, `hey_jarvis_v0.1.onnx`,
`hey_mycroft_v0.1.onnx`, `hey_rhasspy_v0.1.onnx`, `weather_v0.1.onnx`,
`timer_v0.1.onnx` in the upstream catalogue.

## Train your own with openWakeWord

For a custom wakeword see openWakeWord's
[training docs](https://github.com/dscripka/openWakeWord#training-new-models).
The output is an `.onnx` file with input shape `(N, 16, 96)` and output
`(N, 1)` — exactly what horchd's classifier loader expects. Drop it under
`~/.local/share/horchd/models/` and run `horchctl add <name> --model …`.

## In-app training (planned)

The `horchd-gui` Tauri app will eventually ship a Train tab where you
record positive + negative samples through the system mic and produce an
`.onnx` end-to-end. Today that tab is a scaffold — the sample recorder
works, but the training engine itself is not wired up yet. For production
wakewords use one of the two paths above.

## Validation

When you `horchctl add ...`, the daemon loads the `.onnx` and validates the
shape before accepting it. If you see something like:

```
Error: classifier "jarvis" at /…/jarvis.onnx has shape [N, 12, 96] -> [N, 1],
expected (N, 16, 96) -> (N, 1) — was this model trained for openWakeWord?
```

then the model isn't a 16-frame openWakeWord classifier. Re-export it from
your trainer with the right window size.
