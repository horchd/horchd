---
eleventyNavigation:
  key: faq
  title: FAQ
  parent: support
  order: 10
description: "Frequently asked questions about horchd: why a daemon over a library, hardware requirements, license, and comparison with openWakeWord."
---

## Why a daemon? Why not a library?

Most wakeword consumers (Home Assistant bridges, notification scripts,
custom workflows) live in different processes — often different
languages. A daemon broadcasting D-Bus signals lets them all subscribe
from the same source without each spawning their own ONNX session and
duplicating the audio capture.

## Why session bus, not system bus?

System bus access requires a policy file in `/etc/dbus-1/system.d/` plus
running as a privileged service account. The session bus has none of
that — `systemctl --user enable horchd` just works for the logged-in
user. Multi-user setups run one daemon per session.

## Why D-Bus and not a custom socket / HTTP / WebSocket?

D-Bus is already on every Linux desktop, has typed signals, supports
broadcast subscribe semantics natively, and integrates with the systemd
unit lifecycle. No port to choose, no auth to invent, no protocol to
maintain. Multi-platform support (macOS / Windows / Android) will likely
add a localhost-WebSocket transport alongside D-Bus, but on Linux the bus
remains the canonical path.

## How much CPU does it use?

About one CPU **percent** at idle on a modern x86 box. Inference runs at
~12.5 fps (one classifier eval per 80 ms input frame); each eval is under
a millisecond per wakeword model on CPU. Memory is dominated by the
loaded ONNX models (~1.5 MB shared + ~80 KB per wakeword).

## Can I run it without PipeWire?

Yes — cpal also speaks ALSA directly. PipeWire is preferred because it
honours horchd's 16 kHz request without forcing software resampling on
the daemon side. If your hardware only does 44.1 kHz natively, horchd
will currently refuse to start (no software resampler yet); install
PipeWire and the issue goes away.

## How do I debug "no wakeword fires"?

Three usual culprits:

1. **Mic not actually capturing.** `horchctl status` should show
   `audio: ~12.50 fps`. If it's 0, check `pavucontrol`/`pw-top` to make
   sure the daemon's input stream sees real audio.
2. **Threshold too high.** Try `horchctl threshold <name> 0.3`
   (transient) and re-test.
3. **Wrong model.** A model trained for a different wakeword phrase
   won't fire no matter what. Verify with one of the upstream pretrained
   models first (e.g. `hey_jarvis_v0.1.onnx`) to confirm the daemon path
   is healthy, then debug your custom one.

`journalctl --user -fu horchd` is your friend for everything else.

## Will horchd ever support [micro-wake-word](https://github.com/OHF-Voice/micro-wake-word)?

Planned. The audio capture path is engine-agnostic; the inference layer
will grow a `WakewordEngine` trait so a `microwakeword` backend (TFLite
runtime, different feature frontend) can run side-by-side with the
openWakeWord one. Track progress on the issue tracker.

## License?

Dual `MIT OR Apache-2.0`, the standard Rust ecosystem dual-license.
Either is yours to choose.
