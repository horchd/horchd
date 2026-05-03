---
eleventyNavigation:
  key: dbus-api
  title: D-Bus API
  parent: reference
  order: 20
description: "The xyz.horchd.Daemon1 interface on the D-Bus session bus: methods, properties, and the typed Detected signal contract."
---

`horchd` exposes a single object on the **session bus** — not the system
bus. No policy file is needed; the daemon runs as the user.

```
Service:    xyz.horchd.Daemon
Object:     /xyz/horchd/Daemon
Interface:  xyz.horchd.Daemon1
```

The numeric suffix on the interface name is the standard D-Bus convention
for versioned interfaces. A future incompatible change would land as a
parallel `Daemon2` interface so existing clients keep working.

## Methods

| Method          | Args                                                         | Returns        | Notes |
| --------------- | ------------------------------------------------------------ | -------------- | ----- |
| `ListWakewords` | —                                                            | `a(sdsbu)`     | Snapshot of `(name, threshold, model_path, enabled, cooldown_ms)` per entry. |
| `GetStatus`     | —                                                            | `(bddd)`       | `(running, audio_fps, score_fps, mic_level)`. |
| `Add`           | `s name`, `s model_path`, `d threshold`, `u cooldown_ms`     | `()`           | Validates the model (shape + reachability) and appends a `[[wakeword]]` block. Always persists. Errors on duplicate `name` or shape mismatch. |
| `Remove`        | `s name`                                                     | `()`           | Drops the `[[wakeword]]` block; **does not** delete the on-disk model. |
| `SetThreshold`  | `s name`, `d threshold`, `b persist`                         | `()`           | Updates in-memory; `persist=true` writes back to TOML. |
| `SetEnabled`    | `s name`, `b enabled`, `b persist`                           | `()`           | Toggle without unloading. |
| `SetCooldown`   | `s name`, `u ms`, `b persist`                                | `()`           |  |
| `Reload`        | —                                                            | `()`           | Re-read config; diff against in-memory state. Hot-keeps unchanged models; loads new ones; unloads removed ones. Audio thread is preserved. |
| `ListInputDevices` | —                                                         | `as`           | Sorted list of cpal input device names available on the default host. |
| `SetInputDevice`   | `s name`, `b persist`                                     | `()`           | Hot-swap the cpal capture device. `"default"` follows the host default. `persist=true` writes back to TOML. |
| `ProcessAudio`     | `s path`                                                  | `a(sdt)`       | Run all configured wakewords against a 16 kHz mono int16 WAV file off the live mic pipeline. Returns `(name, score, ms_into_file)` per detection. |
| `WyomingStatus`    | —                                                         | `(bsas)`       | `(enabled, mode, listen_uris)` for the embedded Wyoming server. `enabled` reflects current bound state, not just the config flag. |
| `SetWyomingEnabled`| `b enabled`, `b persist`                                  | `b`            | Hot-toggle the Wyoming listener + mDNS at runtime. Returns the bound state after the call. `persist=true` writes `[wyoming].enabled` back to TOML. |

Errors are returned as standard `org.freedesktop.DBus.Error.InvalidArgs` or
`org.freedesktop.DBus.Error.Failed` with a human-readable message.

## Signals

| Signal          | Args                                  | Notes |
| --------------- | ------------------------------------- | ----- |
| `Detected`      | `s name`, `d score`, `t timestamp_us` | Emitted on the rising edge: when a wakeword's score crosses its threshold for the first time within a cooldown window. `timestamp_us` is `CLOCK_MONOTONIC` microseconds since system boot. |
| `ScoreSnapshot` | `s name`, `d score`                   | Low-rate (~5 Hz) per-wakeword score for live UI meters. Always-on; subscribe-time decides cost. |

## Introspection

```bash
busctl --user introspect xyz.horchd.Daemon /xyz/horchd/Daemon xyz.horchd.Daemon1
```

```
NAME            TYPE      SIGNATURE  RESULT/VALUE  FLAGS
.Add            method    sdsbd      —             —
.GetStatus      method    —          bddd          —
.ListWakewords  method    —          a(sdsbu)      —
.Reload         method    —          —             —
.Remove         method    s          —             —
.SetCooldown    method    sub        —             —
.SetEnabled     method    sbb        —             —
.SetThreshold   method    sdb        —             —
.Detected       signal    sdt        —             —
```

## Quick `busctl` calls

```bash
busctl --user call xyz.horchd.Daemon /xyz/horchd/Daemon xyz.horchd.Daemon1 \
       GetStatus
busctl --user call xyz.horchd.Daemon /xyz/horchd/Daemon xyz.horchd.Daemon1 \
       SetThreshold sdb "jarvis" 0.45 false
busctl --user monitor xyz.horchd.Daemon
```
