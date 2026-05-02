---
eleventyNavigation:
  key: configuration
  title: Configuration
  parent: reference
  order: 10
description: "Complete reference for ~/.config/horchd/config.toml — every key, default, and reload semantic of the horchd daemon."
---

`~/.config/horchd/config.toml` is the source of truth. Hand-editable;
`horchctl reload` re-reads it without dropping the audio thread.
`horchctl wakeword {threshold,enable,disable,cooldown,add,remove} --save`
mutate the file in place while preserving comments and formatting.

## Full example

```toml
[engine]
device = "default"           # cpal device name; "default" = system default mic
sample_rate = 16000          # must be 16000; horchd refuses other rates
log_level = "info"           # trace | debug | info | warn | error

[engine.shared_models]
melspectrogram = "/usr/local/share/horchd/melspectrogram.onnx"
embedding      = "/usr/local/share/horchd/embedding_model.onnx"

[[wakeword]]
name = "alexa"                         # unique id; appears in D-Bus signal
model = "~/.local/share/horchd/models/alexa_v0.1.onnx"
threshold = 0.5                        # score must reach this to fire
cooldown_ms = 1500                     # don't refire within this window
enabled = true                         # toggle without unloading

[[wakeword]]
name = "hey_jarvis"
model = "~/.local/share/horchd/models/hey_jarvis_v0.1.onnx"
threshold = 0.65
cooldown_ms = 1500
```

## Field reference

### `[engine]`

| Field         | Type   | Default     | Notes |
| ------------- | ------ | ----------- | ----- |
| `device`      | string | `"default"` | cpal device name. `"default"` = system default mic. |
| `sample_rate` | int    | `16000`     | Must be 16000. Documented sanity check. |
| `log_level`   | string | `"info"`    | `RUST_LOG` env wins if set. |

### `[engine.shared_models]`

| Field            | Type | Required | Notes |
| ---------------- | ---- | -------- | ----- |
| `melspectrogram` | path | yes      | Universal openWakeWord melspec model. |
| `embedding`      | path | yes      | Universal openWakeWord embedding model. |

Both ship with the upstream Python `openwakeword` package and are not
per-wakeword. `install.sh` puts them under `/usr/local/share/horchd/`.

### `[[wakeword]]`

| Field         | Type   | Default | Notes |
| ------------- | ------ | ------- | ----- |
| `name`        | string |  —      | Required. Unique. Appears in the D-Bus `Detected` signal. |
| `model`       | path   |  —      | Required. `.onnx` classifier. `~` expansion supported. |
| `threshold`   | float  | `0.5`   | Score must rise to this for a fire. |
| `cooldown_ms` | int    | `1500`  | Suppress refire within this window. |
| `enabled`     | bool   | `true`  | Toggle without unloading. |

## Multiple aliases on the same model

You can have multiple `[[wakeword]]` entries pointing at the same `model =
"..."` path with different `name`, `threshold`, and `cooldown_ms` values.
Each unique model file is loaded into the ONNX runtime exactly once but the
detector state machine runs per `[[wakeword]]` entry — so N detectors
against M unique models cost M classifier evaluations per frame, not N.
