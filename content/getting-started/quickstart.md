---
eleventyNavigation:
  key: quickstart
  title: Quickstart
  parent: getting-started
  order: 20
description: "Drop a wakeword model into ~/.local/share/horchd/models/, register it in config.toml, and watch it fire on the D-Bus session bus."
---

You've installed horchd (see [Install](/getting-started/install/)) and
`systemctl --user status horchd` says it's `active (running)`. Now register
your first wakeword.

## Option A — use a pretrained openWakeWord model

```bash
# Pull a pretrained model into the user models dir
mkdir -p ~/.local/share/horchd/models
oww=$(python -c 'import openwakeword, pathlib; print(pathlib.Path(openwakeword.__file__).parent / "resources/models")')
cp "$oww/hey_jarvis_v0.1.onnx" ~/.local/share/horchd/models/

# Register it with the daemon
horchctl wakeword add hey_jarvis --model ~/.local/share/horchd/models/hey_jarvis_v0.1.onnx
horchctl status
```

## Option B — train your own

For a custom wakeword see openWakeWord's
[training pipeline](https://github.com/dscripka/openWakeWord#training-new-models).
The export must be `.onnx` with shape `(N, 16, 96) → (N, 1)`. Drop it in the
models dir and register:

```bash
cp ~/Downloads/<name>.onnx ~/.local/share/horchd/models/
horchctl wakeword add <name> --model ~/.local/share/horchd/models/<name>.onnx --threshold 0.5
```

See [Training a wakeword](/guides/training/) for the full path matrix.

## Verify

```bash
horchctl monitor
# (speak the wakeword)
```

Each fire prints one line:

```
0.831423   hey_jarvis              ts=12894301527
```

## React to fires from your own script

Subscribe to `xyz.horchd.Daemon1.Detected` on the session bus from any
language that speaks D-Bus. See the
[Bash](/recipes/bash-subscriber/),
[Python](/recipes/python-subscriber/), and
[Rust](/recipes/rust-subscriber/) subscriber recipes.

## Day-to-day

```bash
horchctl wakeword list                                      # tabular view
horchctl wakeword threshold hey_jarvis 0.6 --save           # tweak + persist
horchctl wakeword disable hey_jarvis --save                 # mute without unloading
horchctl wakeword enable  hey_jarvis --save
horchctl wakeword remove  hey_jarvis                        # keeps the .onnx on disk
horchctl reload                                             # re-read config.toml
horchctl device list                                        # list mic devices
horchctl device set "PipeWire Sound Server"                 # hot-swap input
journalctl --user -fu horchd                                # live logs
```
