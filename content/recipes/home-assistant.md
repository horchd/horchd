---
eleventyNavigation:
  key: home-assistant
  title: Home Assistant
  parent: recipes
  order: 5
description: Plug horchd into Home Assistant's voice pipeline as a Wyoming-protocol wake-word engine. Auto-discovered over mDNS — no IP-typing required.
---

horchd embeds a [Wyoming-protocol](https://github.com/OHF-Voice/wyoming)
listener so Home Assistant's voice pipeline talks to it directly — no
`wyoming-openwakeword` Python bridge in the middle. The integration is
auto-discovered over mDNS (`_wyoming._tcp.local.`) and behaves exactly
like the upstream wake-word engine HA expects.

## 60-second walkthrough

### 1. Enable the Wyoming server in horchd

```bash
horchctl wyoming enable --save
horchctl wyoming status
# enabled: true
# mode:    wyoming-server
# listen:
#   tcp://0.0.0.0:10400
```

`--save` writes `[wyoming].enabled = true` back to `config.toml` so the
choice survives restarts. Drop `--save` if you just want a transient
test session.

For HA's standard voice-pipeline flow you want **`mode = "wyoming-server"`**
— each connecting HA satellite streams its own audio to horchd. If
you're not sure, check:

```bash
grep -A1 '\[wyoming\]' ~/.config/horchd/config.toml | grep mode
```

If you see `mode = "local-mic"` or no line at all, edit the file:

```toml
# ~/.config/horchd/config.toml
[wyoming]
enabled = true
mode = "wyoming-server"
listen = ["tcp://0.0.0.0:10400"]
zeroconf = true
```

…and `horchctl reload` (or restart the daemon — `mode` changes apply
to new connections only).

### 2. Verify the wire

```bash
echo '{"type":"describe"}' | nc -q1 127.0.0.1 10400
```

You should get back two lines: a Wyoming header, then a JSON
`info` event listing every wakeword you've registered. If that works,
HA can talk to it.

### 3. Add the integration in Home Assistant

1. **Settings → Devices & services → Add Integration**
2. Search for **Wyoming Protocol**
3. horchd should be auto-discovered as `horchd-<hostname>`. Click it.
4. If discovery is blocked on your network, pick "Manual" and enter the
   horchd host's IP and port `10400`.

The integration creates a wake-word entity per registered wakeword.

### 4. Wire it into a Voice Assistant pipeline

1. **Settings → Voice assistants** → existing pipeline (or new)
2. **Wake word engine** → pick `horchd`
3. **Wake word** → pick the entity for the wakeword you want
   (e.g. `alexa`)
4. Save.

Now: speak the wakeword to any HA satellite (HA Voice PE, ESP32-S3-BOX,
the HA Companion app's wake-word forwarder, etc.). The satellite streams
audio to horchd over Wyoming, horchd detects, replies with a `detection`
event, and HA's pipeline transitions into STT.

## Modes — pick the right one

| `[wyoming].mode` | Audio source | Use case |
| --- | --- | --- |
| `wyoming-server` | each HA satellite streams its own audio | **Standard HA voice pipeline.** Per-connection isolated inference state — multiple satellites don't interfere. |
| `local-mic` | the daemon's local microphone | "horchd at my desk, broadcast wakewords to HA as generic events". Client `audio-chunk`s are ignored. |
| `hybrid` | both at once | Local mic AND remote satellites. Maximum flexibility, doubled inference cost. |

The default is `local-mic` — if you want HA to actually drive the
audio (which is what their voice pipeline expects), switch to
`wyoming-server`.

## Audio format expectations

v1 only accepts the openWakeWord canonical format:

- 16 000 Hz
- 1 channel (mono)
- 16-bit signed (`width = 2`)

Every shipping HA Wyoming satellite already emits this — Voice PE,
ESP32-S3-BOX, Wyoming-Satellite, the Companion-app forwarder. If you
see logs like `Wyoming client offered audio at 48000 Hz; horchd needs
16000 Hz`, the satellite is misconfigured (force the 16 kHz pipeline
in its YAML / settings). Resampling is on the roadmap but not wired
yet.

## Troubleshooting

### HA doesn't discover horchd

- mDNS is blocked on your network. Try the manual IP entry instead.
- Confirm the listener is up: `horchctl wyoming status` and
  `ss -tlnp | grep 10400`.
- Check the daemon log: `journalctl --user -u horchd -f`. You should
  see `Wyoming TCP listening` and `Wyoming mDNS announced` lines.

### HA discovers it but no detections fire

- Wrong mode. Check `horchctl wyoming status` — must be
  `wyoming-server` (or `hybrid`) for HA to actually drive the audio.
- Wakeword filter mismatch. The HA integration sends a `detect` event
  with the wakeword names it cares about. If those names don't match
  any of your `[[wakeword]] name = "..."` entries, nothing fires.
  `horchctl wakeword list` to see what's registered.
- Satellite sending the wrong audio format. Daemon log will say so.

### Live monitoring

```bash
horchctl monitor
# 0.834567   alexa                     ts=12345678901
```

Note: `horchctl monitor` reads the D-Bus `Detected` signal — that fires
for the **local mic pipeline** (`local-mic` and `hybrid` modes). In
pure `wyoming-server` mode the local mic isn't running, so D-Bus stays
silent; the detection goes back to the HA satellite over Wyoming
instead.
