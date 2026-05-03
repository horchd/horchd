# Bundled openWakeWord shared models

The two ONNX files in `/usr/local/share/horchd/`:

- `melspectrogram.onnx`
- `embedding_model.onnx`

are unmodified copies from the
[openWakeWord v0.5.1](https://github.com/dscripka/openWakeWord/releases/tag/v0.5.1)
GitHub release, licensed under the **Apache License 2.0**.

Download URLs:

- <https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/melspectrogram.onnx>
- <https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/embedding_model.onnx>

These files are the universal preprocessing models every openWakeWord
classifier expects. They are not horchd's own work and are redistributed
unchanged so the container is self-contained at first run. Upstream's
own Python package downloads from the same URLs at install time.
