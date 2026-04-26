# horchd

A native Linux daemon that listens to the system microphone, detects
user-defined wakewords in parallel, and broadcasts a D-Bus signal the
moment any of them fires. Other apps (Home Assistant, custom scripts,
notification daemons, …) subscribe to that signal and react.

- **Native**: a single Rust binary (~6 MB) plus the ONNX Runtime shared library
- **Multi-wakeword**: one openWakeWord-style `.onnx` per wake; runs them all on the same audio
- **D-Bus first**: anything that speaks the session bus can subscribe — no HTTP, no custom protocol
- **systemd user unit**: no root needed, no system-bus policy file
- **Trainer-agnostic**: ships with the upstream openWakeWord pretrained models;
  bring your own `.onnx` from [Lyna](https://github.com/horchd/lyna) or any
  other openWakeWord-compatible trainer

```
cpal mic 16 kHz mono
  → 80 ms / 1280-sample frames
  → melspectrogram.onnx                  (universal)
  → embedding_model.onnx                 (universal, 96-dim per 80 ms)
  → sliding window of last 16 embeddings (1.28 s receptive field)
  → fan-out to per-wakeword classifier   (input (1, 16, 96), output f32 in [0,1])
  → threshold + cooldown state machine
  → D-Bus Detected(name, score, timestamp_us) signal
```

## Install

```bash
git clone https://github.com/horchd/horchd
cd horchd
# Drop the shared models in shared-models/ first — see below
./packaging/install.sh
```

`install.sh` does:
- `cargo build --release` of `horchd` and `horchctl`
- copies binaries to `/usr/local/bin/` (sudo)
- copies `shared-models/*.onnx` to `/usr/local/share/horchd/` (sudo)
- seeds `~/.config/horchd/config.toml` from `examples/horchd.toml`
  (only if the file doesn't yet exist — re-runnable)
- installs the systemd **user** unit and runs `systemctl --user enable --now horchd`

### Shared models

The `melspectrogram.onnx` and `embedding_model.onnx` files are bundled with
the [openwakeword](https://github.com/dscripka/openWakeWord) Python package.
Drop them into `shared-models/` before installing:

```bash
oww=$(python -c 'import openwakeword, pathlib; print(pathlib.Path(openwakeword.__file__).parent / "resources/models")')
cp "$oww"/{melspectrogram,embedding_model}.onnx shared-models/
```

## First wakeword

```bash
# Pull a pretrained openWakeWord model into the user models dir
mkdir -p ~/.local/share/horchd/models
cp "$oww"/hey_jarvis_v0.1.onnx ~/.local/share/horchd/models/

# Register it with the daemon
horchctl add hey_jarvis --model ~/.local/share/horchd/models/hey_jarvis_v0.1.onnx
horchctl status
horchctl monitor   # speak "hey jarvis"
```

For a custom wakeword, train one in [Lyna](https://github.com/horchd/lyna),
drop the resulting `<name>.onnx` into `~/.local/share/horchd/models/`, and
`horchctl add <name> --model ...`.

## CLI cheatsheet

```bash
horchctl status                                   # daemon health + loaded wakewords
horchctl list                                     # tabular view
horchctl monitor                                  # tail Detected signals live

horchctl threshold lyna 0.45                      # transient (resets on restart)
horchctl threshold lyna 0.45 --save               # persist to config.toml (keeps comments)
horchctl cooldown lyna 1200 --save
horchctl enable lyna --save
horchctl disable lyna --save

horchctl add wetter --model ~/.local/share/horchd/models/wetter.onnx --threshold 0.55
horchctl remove wetter            # keeps the .onnx on disk
horchctl remove wetter --purge    # also deletes the .onnx + .onnx.data sibling

horchctl reload                   # re-read config.toml; hot-keep unchanged models
```

## Subscribing from your own code

The D-Bus interface is `xyz.horchd.Daemon1` at object path
`/xyz/horchd/Daemon` on the **session** bus. The signal is:

```
Detected(s name, d score, t timestamp_us)
```

Subscriber examples for bash/python/rust live under
[`examples/`](examples/). The Rust trait that `horchctl` uses is
re-exported from `horchd-core` if you want to embed the proxy in your own
crate.

## Configuration

`~/.config/horchd/config.toml` is the source of truth. Hand-editable;
`horchctl reload` picks up changes without dropping the audio thread.

```toml
[engine]
device = "default"
sample_rate = 16000
log_level = "info"

[engine.shared_models]
melspectrogram = "/usr/local/share/horchd/melspectrogram.onnx"
embedding      = "/usr/local/share/horchd/embedding_model.onnx"

[[wakeword]]
name = "lyna"
model = "~/.local/share/horchd/models/lyna.onnx"
threshold = 0.5      # default
cooldown_ms = 1500   # default
enabled = true       # default
```

## Logs

```bash
journalctl --user -fu horchd
```

## License

Dual `MIT OR Apache-2.0`. See `LICENSE-MIT` and `LICENSE-APACHE`.
