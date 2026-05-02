---
eleventyNavigation:
  key: process-audio
  title: Process recorded audio
  parent: recipes
  order: 40
description: "Run all configured wakewords against a WAV file off the live mic pipeline. Useful for CI regression tests, audit replays, and false-positive debugging."
---

`horchctl process FILE.wav` runs every configured wakeword against an
audio file. The daemon spins up a separate isolated inference state for
the call — your live mic stream isn't disturbed, and detections from
the file aren't broadcast to D-Bus subscribers (they're returned to
`horchctl` and printed).

## Use cases

- **Regression-test wakeword models** — keep a curated set of "should
  fire" and "should not fire" recordings in CI; a model update that
  shifts the score envelope is caught immediately.
- **Audit past detections** — record a couple of hours of mic input
  alongside the daemon's `journalctl`, then replay the file later to
  confirm whether a flagged Detection actually had the wakeword in it.
- **Debug false positives** — capture a moment when the daemon fired
  unexpectedly, replay it with `--threshold-override` (planned) to
  characterize the score curve.

## Audio format

The WAV must be 16 kHz mono int16. If yours isn't, convert:

```bash
ffmpeg -i in.flac -ar 16000 -ac 1 -sample_fmt s16 fixed.wav
```

## Human-readable output (default)

```bash
$ horchctl process tests/alexa-utterance.wav
    0.320s  alexa                 score=0.974
    1.840s  alexa                 score=0.812
```

## JSONL output (for `jq`, CI)

```bash
$ horchctl process tests/alexa-utterance.wav --json | jq
{
  "timestamp_s": 0.32,
  "name": "alexa",
  "score": 0.974
}
{
  "timestamp_s": 1.84,
  "name": "alexa",
  "score": 0.812
}
```

Assert in CI that a known-good utterance fires:

```bash
horchctl process fixtures/alexa.wav --json \
  | jq -e 'select(.name == "alexa" and .score > 0.5)' >/dev/null \
  || { echo "alexa regression"; exit 1; }
```

## Isolation guarantee

The file pipeline owns its own `Preprocessor` + `Classifier` set
(loaded fresh from the same config paths as the live mic pipeline) and
its own detector state. Two consequences:

1. The live mic pipeline keeps streaming at full FPS during the call —
   confirmed by watching `horchctl status` while a long file processes.
2. Each `horchctl process` invocation pays a ~200 ms one-time setup
   cost for loading the ONNX sessions. For sub-second files that can
   dominate end-to-end time; for multi-second files it's lost in the
   inference cost.

## Cooldown caveat

The detector's cooldown (`cooldown_ms`) is wall-clock-based. When a
file processes faster than realtime — typical, since 1 s of audio takes
~80 ms of CPU — two utterances that are 1.5 s apart in the recording
may collapse to ~120 ms apart in wall time, fall inside the cooldown
window, and the second one is dropped. Acceptable for v0.2.0; v0.3.0
plans a virtual-time refactor of the detector for this exact case.
