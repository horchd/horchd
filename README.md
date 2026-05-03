# horchd

> A native Linux daemon that listens to the system microphone, detects
> any of *N* user-defined wakewords in parallel, and broadcasts a D-Bus
> `Detected` signal the moment one fires.

[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![docs](https://img.shields.io/badge/docs-docs.horchd.xyz-c8311c)](https://docs.horchd.xyz)

`horchd` is a tiny Rust daemon (≈6 MB binary, ~1% CPU at idle) that
ports the [openWakeWord](https://github.com/dscripka/openWakeWord)
inference pipeline to ONNX Runtime + Rust and exposes it over the
**session** D-Bus. Anything that speaks D-Bus — Home Assistant
bridges, notification scripts, an MPRIS client, the bundled
[`horchctl`](#horchctl) CLI, the
[`horchd-gui`](#horchd-gui) tray app — can subscribe to wake events
without re-implementing audio capture or inference.

It's a drop-in replacement for the Python
[openwakeword](https://github.com/dscripka/openWakeWord) runtime when
you want a native daemon instead of a Python process, and loads any
[openWakeWord](https://github.com/dscripka/openWakeWord)-compatible
`.onnx` classifier (1, 16, 96) → (1, 1).

---

## Highlights

- **Daemon-first** — `horchd` is a standalone Linux service; everything
  else (`horchctl`, `horchd-gui`, your scripts) is an independent D-Bus
  client. No client is required to run the daemon, no client links
  against the daemon — they all just speak `xyz.horchd.Daemon1`.
- **Native** — single Rust 2024 binary; ONNX Runtime as the only big dep
- **CPU-only inference** — fits on a single core; no CUDA / ROCm / GPU runtime
- **Multi-wakeword** — N classifiers run on the same audio frame, fan-out at the embedding stage
- **Cheap** — ~12.5 inferences/sec on one core, ~1.5 MB shared models + ~80 KB per wakeword
- **D-Bus first** — no HTTP listener, no custom socket, no cloud
- **Wyoming-protocol server** — drop-in for `wyoming-openwakeword`, auto-discovered by Home Assistant via mDNS
- **systemd user unit** — no root, no system-bus policy file
- **Hot-reload** — edit the TOML, `horchctl reload`, never drops the audio thread
- **Trainer-agnostic** — bring any
  [openWakeWord](https://github.com/dscripka/openWakeWord)-compatible
  `.onnx` classifier; `horchctl wakeword import` pulls from any URL or local path
- **Future**: dual-engine support for [`micro-wake-word`](https://github.com/OHF-Voice/micro-wake-word) (the engine ESPHome / Home Assistant Voice uses) — see [roadmap](#roadmap)

## How it works

```
cpal microphone (PipeWire / PulseAudio / ALSA)
  │   16 kHz mono
  ▼
80 ms / 1280-sample frames
  │
  ▼
melspectrogram.onnx                   ← bundled, universal
  │   8 mel frames per 80 ms input (32 bins, 10 ms hop)
  ▼
embedding_model.onnx                  ← bundled, universal
  │   one 96-dim embedding per 80 ms
  ▼
sliding window of last 16 embeddings  (≈1.28 s receptive field)
  │
  ▼  fan-out
┌─────┬──────┬──────┬─────┐
▼     ▼      ▼      ▼     ▼
jarvis wetter alexa …    per-wakeword .onnx classifier  →  score in [0,1]
  │
  ▼  rising-edge detector + per-wake cooldown
xyz.horchd.Daemon1.Detected(name, score, timestamp_us)
  │
  ├──▶ horchctl monitor
  ├──▶ horchd-gui ticker
  ├──▶ Home Assistant bridge
  └──▶ your script
```

The 3-stage pipeline (melspec → embedding → classifier) is exactly
what [`openwakeword.Model.predict()`](https://github.com/dscripka/openWakeWord/blob/main/openwakeword/model.py)
does internally — `horchd` ports it to Rust + [ort](https://crates.io/crates/ort)
so it can run as a long-lived daemon instead of a per-request Python
process.

## Performance

Microbenchmarks for the two hot paths that aren't dominated by ONNX
runtime cost: the cpal-callback (downmix + decimation + peak EMA + frame
emit) and the per-wakeword detector state machine. ONNX inference itself
is bounded by `ort` and the bundled openWakeWord models — measured live
via the daemon's `mean_latency_us` / `max_latency_us` counters and
exposed in the `stats` log line every 30 s.

![horchd hot-path latency](https://quickchart.io/chart?c=%7B%22type%22%3A%22bar%22%2C%22data%22%3A%7B%22labels%22%3A%5B%22Detector+update%22%2C%22callback+mono+1280%22%2C%22callback+stereo+3840+d%3D3%22%5D%2C%22datasets%22%3A%5B%7B%22label%22%3A%22microseconds%22%2C%22data%22%3A%5B0.00643%2C2.85%2C8.34%5D%2C%22backgroundColor%22%3A%22%23c8311c%22%2C%22borderColor%22%3A%22%231a1a1a%22%2C%22borderWidth%22%3A1%7D%5D%7D%2C%22options%22%3A%7B%22indexAxis%22%3A%22y%22%2C%22plugins%22%3A%7B%22title%22%3A%7B%22display%22%3Atrue%2C%22text%22%3A%22horchd+hot-path+latency+%28microseconds%2C+log+scale%2C+lower+is+better%29%22%2C%22color%22%3A%22%231a1a1a%22%2C%22font%22%3A%7B%22size%22%3A16%7D%7D%2C%22legend%22%3A%7B%22display%22%3Afalse%7D%7D%2C%22scales%22%3A%7B%22x%22%3A%7B%22type%22%3A%22logarithmic%22%2C%22min%22%3A0.001%2C%22max%22%3A30%2C%22ticks%22%3A%7B%22color%22%3A%22%231a1a1a%22%7D%2C%22grid%22%3A%7B%22color%22%3A%22%23e8e4d6%22%7D%2C%22title%22%3A%7B%22display%22%3Atrue%2C%22text%22%3A%22microseconds+%28log+scale%29%22%2C%22color%22%3A%22%231a1a1a%22%7D%7D%2C%22y%22%3A%7B%22ticks%22%3A%7B%22color%22%3A%22%231a1a1a%22%7D%2C%22grid%22%3A%7B%22display%22%3Afalse%7D%7D%7D%7D%7D&bkg=%23fafaf6&w=880&h=300&v=4)

| Path                                  |  Latency  | Cost @ 12.5 fps |
| ------------------------------------- | --------: | --------------: |
| `Detector::update` (steady-state)     |   6.43 ns |        0.000 %  |
| audio callback · mono · 1280 samples  |   2.85 µs |        0.004 %  |
| audio callback · stereo · 3840 (d=3)  |   8.34 µs |        0.010 %  |

Real-world budget at the daemon's 12.5 fps frame rate:
**12.5 × 8.34 µs ≈ 0.01 % of one CPU core for audio capture**, plus
~13 ns/frame for every detector. Inference runs on the CPU execution
provider — that's where the rest of the budget goes.

Numbers above were measured on:

- **CPU**: AMD Ryzen 7 9800X3D (8 cores / 16 threads, 3D V-Cache)
- **RAM**: 32 GiB DDR5
- **OS**: CachyOS Linux, kernel 7.0.2
- **Toolchain**: rustc 1.94.1, profile `release` (LTO thin, codegen-units = 1)

Release-binary footprint on the same host:

| Binary                  | Size    | Notes                                                                                                      |
| ----------------------- | ------- | ---------------------------------------------------------------------------------------------------------- |
| `horchd` (default)      | 28.9 MB | self-contained — `ort` feature `download-binaries` ships libonnxruntime in the binary                      |
| `horchd` (`dynamic`)    |  5.2 MB | `--no-default-features --features dynamic-onnxruntime`; needs `libonnxruntime.so` on the system at runtime |
| `horchctl`              |  8.5 MB | reqwest + rustls (no openssl), clap, zbus, sha2                                                            |

For distro packages (Arch / .deb / .rpm) build the dynamic variant and
add a `Depends: onnxruntime` line; for one-shot `cargo install` the
default keeps everything in one self-contained binary.

Reproduce on your hardware:

```bash
./scripts/bench.sh        # writes target/criterion/report/index.html
./scripts/coverage.sh     # writes target/llvm-cov/html/index.html
```

`scripts/bench.sh` uses [Criterion](https://github.com/bheisler/criterion.rs)
and produces statistical confidence intervals; `scripts/coverage.sh`
auto-installs `cargo-llvm-cov` on first run.

## Repository layout

```
horchd/
├── crates/
│   ├── client/          shared types + D-Bus proxy trait (consumed by every binary)
│   ├── horchd/          the daemon (lib + bin)
│   ├── horchctl/        CLI client (status, list, monitor, threshold, add, remove, reload, …)
│   └── gui/             Tauri 2 tray + control panel (SvelteKit + Tailwind v4)
├── python/            optional training helper (subprocessed by the GUI's Train tab)
├── shared-models/     melspectrogram.onnx + embedding_model.onnx (gitignored)
├── systemd/           user unit
├── packaging/         install.sh + arch/PKGBUILD
├── examples/          horchd.toml + subscriber.{sh,py}
└── scripts/           check.sh, coverage.sh, bench.sh
```

## Install

### From source

```bash
git clone https://codeberg.org/NewtTheWolf/horchd
cd horchd

# Drop the universal preprocessing models (one-time setup)
oww=$(python -c 'import openwakeword, pathlib; print(pathlib.Path(openwakeword.__file__).parent / "resources/models")')
cp "$oww"/{melspectrogram,embedding_model}.onnx shared-models/

./packaging/install.sh
```

`install.sh` builds release binaries, installs them to `/usr/local/bin/`,
copies the shared models to `/usr/local/share/horchd/`, scaffolds
`~/.config/horchd/config.toml` and `~/.local/share/horchd/models/`,
then enables and starts the systemd user unit.

Verify:

```bash
systemctl --user status horchd
horchctl status
```

### Arch / CachyOS (AUR)

A `PKGBUILD` ships under [`packaging/arch/`](packaging/arch/PKGBUILD).
Once published it'll be:

```bash
yay -S horchd       # paru / makepkg also fine
```

The package depends on `pipewire`. After install, drop the shared models
into `/usr/share/horchd/` and `systemctl --user enable --now horchd`.

### Docker

For headless wyoming-server deployments (HA voice pipeline talks to
horchd over TCP, no host audio device needed):

```bash
docker run --rm -d \
    --name horchd \
    -p 10400:10400 \
    -v horchd-data:/data \
    ghcr.io/newtthewolf/horchd:latest

# Import wakewords inside the container:
docker exec -it horchd horchctl wakeword import \
    https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/alexa_v0.1.onnx
```

A ready-to-run compose file lives at
[`docker/compose.example.yml`](./docker/compose.example.yml).

The container ships with `engine.local_mic = false` baked in — the
daemon serves Wyoming clients only, no cpal mic open. If you want both
local mic AND container, expose `/dev/snd` and mount the PipeWire
socket; that's not a primary use case but documented in the compose
file's comments.

The two openWakeWord shared models (`melspectrogram.onnx`,
`embedding_model.onnx`) are baked into the image (Apache-2.0
attribution at `/usr/local/share/horchd/ATTRIBUTION.md`). Bring your
own classifier `.onnx` files via the `/data` volume (default models
dir inside the container = `/data/horchd/models/`).

Multi-arch image: `linux/amd64` and `linux/arm64` (no armv7 — HA
Supervisor 2026.03 dropped it).

### Nix

Out of scope for v0.1. PRs welcome.

## First wakeword (60-second walkthrough)

```bash
# Pull a pretrained openWakeWord model into the user models dir
mkdir -p ~/.local/share/horchd/models
oww=$(python -c 'import openwakeword, pathlib; print(pathlib.Path(openwakeword.__file__).parent / "resources/models")')
cp "$oww/hey_jarvis_v0.1.onnx" ~/.local/share/horchd/models/

# Register it with the daemon (validates shape, loads it, persists to TOML)
horchctl wakeword add hey_jarvis --model ~/.local/share/horchd/models/hey_jarvis_v0.1.onnx

# Verify
horchctl status
horchctl monitor      # speak "hey jarvis"
```

For a custom wakeword you currently have one supported path:

1. **External training** — produce an
   [openWakeWord](https://github.com/dscripka/openWakeWord)-compatible
   classifier (input `(1, 16, 96)`, output `(1, 1)`) yourself, drop the
   `.onnx` into `~/.local/share/horchd/models/`, and
   `horchctl wakeword add <name> --model …`. Any pipeline that exports a model
   matching that shape will work; the daemon validates it at register-time.

The **in-app Train tab in `horchd-gui`** wraps the bundled
`python/horchd_train` package and the upstream openWakeWord trainer to
record + augment + fit + export end-to-end, but is **work-in-progress**
— several stages of the Python pipeline still have known bugs (dtype
mismatches, label-key handling, `pyroomacoustics` dep). Tracked under
[roadmap](#roadmap); use external training in the meantime.

## D-Bus API

The daemon's only contract — anything below (`horchctl`, `horchd-gui`,
your scripts) is just a client of this surface.

Bus: **session bus**. No system-bus policy, runs as the user.

```
Service:    xyz.horchd.Daemon
Object:     /xyz/horchd/Daemon
Interface:  xyz.horchd.Daemon1
```

The trailing `1` in the interface name is D-Bus convention for "version
1 of this interface" (cf. `org.freedesktop.systemd1.Manager`,
`org.freedesktop.Tracker3.Endpoint`). Backwards-incompatible changes
ship as a parallel `Daemon2` so old clients keep working until they
migrate.

| Method               | Args                                                     | Returns      |
| -------------------- | -------------------------------------------------------- | ------------ |
| `ListWakewords`      | —                                                        | `a(sdsbu)`   |
| `GetStatus`          | —                                                        | `(bddd)`     |
| `Add`                | `s name`, `s model_path`, `d threshold`, `u cooldown_ms` | `()`         |
| `Remove`             | `s name`                                                 | `()`         |
| `SetThreshold`       | `s name`, `d threshold`, `b persist`                     | `()`         |
| `SetEnabled`         | `s name`, `b enabled`, `b persist`                       | `()`         |
| `SetCooldown`        | `s name`, `u ms`, `b persist`                            | `()`         |
| `ListInputDevices`   | —                                                        | `as`         |
| `SetInputDevice`     | `s name`, `b persist`                                    | `()`         |
| `Reload`             | —                                                        | `()`         |
| `WyomingStatus`      | —                                                        | `(bsas)`     |
| `SetWyomingEnabled`  | `b enabled`, `b persist`                                 | `b`          |

`GetStatus` returns `(running, audio_fps, score_fps, mic_level)` — the
trailing `mic_level` is the smoothed peak `|sample|` of the most recent
cpal callback in `[0, 1]`, used by the GUI mic meter.

`Add` validates that the supplied `.onnx` lives under the canonical
models directory (`$XDG_DATA_HOME/horchd/models/` or
`~/.local/share/horchd/models/`) and rejects anything else, so a
session-bus client cannot point the daemon at arbitrary files.

`WyomingStatus` returns `(enabled, mode, listen_uris)` for the embedded
[Wyoming](https://github.com/OHF-Voice/wyoming) protocol server — see
the next section.

| Signal          | Args                                  | Notes |
| --------------- | ------------------------------------- | ----- |
| `Detected`      | `s name`, `d score`, `t timestamp_us` | Rising-edge fire after threshold + cooldown. |
| `ScoreSnapshot` | `s name`, `d score`                   | ~5 Hz per-wakeword score; for live UI meters. |

Full reference + introspection output: <https://docs.horchd.xyz/reference/dbus-api/>.

## Wyoming server (Home Assistant)

horchd embeds a [Wyoming-protocol](https://github.com/OHF-Voice/wyoming)
listener so Home Assistant's voice pipeline (and any other Wyoming
client) talks to it directly — no bridge daemon, no
`wyoming-openwakeword` Python service in the middle.

Off by default. Two ways to enable:

```bash
# Hot toggle, no daemon restart. --save also writes back to config.toml.
horchctl wyoming enable --save
horchctl wyoming status
horchctl wyoming disable           # transient; comes back on next start
```

…or by editing the config file directly:

```toml
# ~/.config/horchd/config.toml
[wyoming]
enabled = true
mode = "wyoming-server"                  # see "Modes" below
listen = ["tcp://0.0.0.0:10400"]
zeroconf = true                          # advertise _wyoming._tcp.local.
# service_name = "horchd-living"         # default = "horchd-<hostname>"
```

Verify the listener answers:

```bash
echo '{"type":"describe"}' | nc -q1 127.0.0.1 10400
```

The second line returns an `info` event listing every wakeword you've
registered. Add the daemon as a Wyoming integration in Home Assistant —
mDNS auto-discovery should surface it as `horchd-<hostname>` on
`_wyoming._tcp.local.`. If discovery is blocked on your network, point
HA at `<host>:10400` manually.

### Modes

| `mode` | Audio source | Use case |
| --- | --- | --- |
| `local-mic` (default) | the daemon's local mic | "horchd at my desk, broadcast wakewords to HA as events"; client `audio-chunk`s are ignored |
| `wyoming-server` | each client streams its own audio via `audio-chunk`s | drop-in replacement for `wyoming-openwakeword`; standard HA voice-pipeline topology |
| `hybrid` | both — local mic + client-streamed audio | runs both flows side by side |

In `wyoming-server` and `hybrid` mode horchd loads a fresh isolated
inference state per connection on the first `audio-start` (~200 ms,
~10 MB extra RAM per client) so multiple clients don't interfere with
each other. v1 only accepts the openWakeWord canonical 16 kHz / mono /
int16 input format — that's what every shipping HA Wyoming satellite
emits. Off-spec audio is rejected with an actionable message.

## Subscribers

Anything that speaks D-Bus is a valid subscriber. From the shell:

```bash
busctl --user monitor xyz.horchd.Daemon
gdbus monitor --session --dest xyz.horchd.Daemon --object-path /xyz/horchd/Daemon
```

### Rust

The [`horchd-client`](crates/client/) crate ships the zbus proxy
trait + the on-the-wire types (`Config`, `Wakeword`, `WakewordEvent`,
`WakewordSnapshot`, …). Every binary in this repo uses it; external
projects use the same crate.

```rust
use futures_util::StreamExt;
use horchd_client::DaemonProxy;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let conn = zbus::Connection::session().await?;
    let proxy = DaemonProxy::new(&conn).await?;

    // One-shot calls
    let (running, audio_fps, score_fps, mic_level) = proxy.get_status().await?;
    println!("daemon running={running}, audio_fps={audio_fps:.1}, mic={mic_level:.2}");

    // Subscribe to the Detected signal stream
    let mut stream = proxy.receive_detected().await?;
    while let Some(sig) = stream.next().await {
        let args = sig.args()?;
        println!("fired: {} score={:.3}", args.name, args.score);
    }
    Ok(())
}
```

Bash + Python (`dbus-next`) examples live under [`examples/`](examples/).

## horchctl (optional)

The reference CLI client. Same D-Bus surface, no extra privileges.

```bash
horchctl status                                   # daemon health + loaded wakewords
horchctl wakeword list                                     # tabular view
horchctl monitor                                  # tail Detected signals live

horchctl wakeword threshold jarvis 0.45                    # transient (resets on restart)
horchctl wakeword threshold jarvis 0.45 --save             # persist to config.toml (preserves comments)
horchctl wakeword cooldown  jarvis 1200 --save
horchctl wakeword enable    jarvis --save
horchctl wakeword disable   jarvis --save

horchctl wakeword add wetter --model ~/.local/share/horchd/models/wetter.onnx --threshold 0.55
horchctl wakeword remove wetter            # keeps the .onnx on disk
horchctl wakeword remove wetter --purge    # also deletes the .onnx + .onnx.data sibling

horchctl reload                   # re-read config.toml; hot-keep unchanged models

# import a model from a URL or local path; stages it under ~/.local/share/horchd/models/
horchctl wakeword import https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/alexa_v0.1.onnx
horchctl wakeword import ~/Downloads/my-model.onnx --as my_wake --threshold 0.65
horchctl wakeword import https://example.com/m.onnx --as wake --force   # re-download + re-register

horchctl process recording.wav         # run wakewords against a file (human output)
horchctl process recording.wav --json  # one JSON object per detection, jq-friendly

horchctl device list                            # what input devices does cpal see?
horchctl device set "PipeWire Sound Server"     # transient hot-swap
horchctl device set default --save              # persist to config.toml

horchctl wyoming status                         # is the Wyoming server up? which URIs?
horchctl wyoming enable --save                  # bind listeners now + persist
horchctl wyoming disable                        # stop listeners (transient)
```

All mutator commands either error out cleanly (validates shape /
unique name / cooldown) or echo back what they did. `--save` writes
through [`toml_edit`](https://crates.io/crates/toml_edit) so user
comments and ordering survive.

### Process recorded audio

`horchctl process FILE.wav` runs every configured wakeword against an
audio file off the live mic pipeline. The daemon spins up a separate
isolated inference state for the call — your live mic stream isn't
disturbed, and detections from the file aren't broadcast to D-Bus
subscribers (they're returned to `horchctl` and printed).

WAV must be 16 kHz mono int16. Convert with:
```bash
ffmpeg -i in.flac -ar 16000 -ac 1 -sample_fmt s16 out.wav
```

JSON output for CI / `jq` pipes:
```bash
$ horchctl process tests/alexa-utterance.wav --json | jq
{
  "timestamp_s": 0.32,
  "name": "alexa",
  "score": 0.974
}
```

Use cases: regression-testing wakeword models against curated recordings,
auditing past Detections, debugging false positives by replaying the
suspect audio.

## horchd-gui (optional)

A Tauri 2 tray app + control panel for users who'd rather not live in
the terminal. Independent D-Bus client of the daemon — no extra
privileges, no link against `horchd` or `horchctl`.

Status: **Status / Wakewords / Settings tabs are functional**. The
**Train tab is work-in-progress** — the recording + storage paths work,
but the bundled Python trainer (`python/horchd_train`) still has known
bugs that prevent end-to-end training; see [roadmap](#roadmap).

Stack: **SvelteKit** (Svelte 5 runes) + **Tailwind v4** + **@lucide/svelte** +
**Bun**. Design language: brutalist scientific instrument — warm
parchment background, ink-black text, one signal-red accent for fires,
**Fraunces** italic serif for hero numerals and the wordmark, **IBM
Plex Mono** for everything else, hairline borders instead of rounded
corners.

```bash
cd crates/gui

# Dev: vite dev server + Tauri shell with HMR (auto-spawns both)
cargo tauri dev

# Production native bundle (.deb / .rpm / .AppImage)
cargo tauri build
```

The crate follows the canonical Tauri 2 + SvelteKit layout — frontend
at `crates/gui/`, Rust + `tauri.conf.json` at
`crates/gui/src-tauri/`. Bootstrap (Linux dev headers + Tauri
CLI) is in [crates/gui/README.md](crates/gui/README.md).

The Wayland workaround for `Gdk Error 71` (NVIDIA + webkit2gtk +
Wayland) is set automatically inside the binary; nothing to configure.

## Configuration

`~/.config/horchd/config.toml`. Hand-editable; `horchctl reload`
re-reads without dropping the audio thread.

```toml
[engine]
device = "default"           # cpal device name; "default" = system default mic
sample_rate = 16000          # must be 16000; horchd refuses other rates
log_level = "info"

[engine.shared_models]
melspectrogram = "/usr/local/share/horchd/melspectrogram.onnx"
embedding      = "/usr/local/share/horchd/embedding_model.onnx"

[[wakeword]]
name = "jarvis"              # appears in the D-Bus signal
model = "~/.local/share/horchd/models/jarvis.onnx"
threshold = 0.5              # default
cooldown_ms = 1500           # default
enabled = true               # default
```

Field reference: <https://docs.horchd.xyz/reference/configuration/>.

## Develop

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUST_LOG=horchd=debug cargo run --bin horchd -- --config examples/horchd.toml
```

Helpers under [`scripts/`](scripts/):

```bash
./scripts/check.sh       # fmt + clippy + tests + frontend type-check
./scripts/coverage.sh    # cargo-llvm-cov HTML report (auto-installs)
./scripts/bench.sh       # Criterion benchmarks → target/criterion/report
```

Benchmarks live under `crates/horchd/benches/` (Criterion). The
detector state machine and cpal-callback hot path are benchmarked so
regressions in either show up immediately.

GUI dev:

```bash
cd crates/gui
bun install
bun run dev          # vite dev server with HMR on :5173
# In another terminal: cargo tauri dev   (loads http://localhost:5173)
```

The SvelteKit page also runs **standalone in any browser** against a
deterministic mock backend, so you can iterate on the design without a
running daemon (or even Tauri):

```bash
cd crates/gui
bun run dev
xdg-open http://localhost:5173
```

The mock kicks in automatically when `window.__TAURI_INTERNALS__` is
absent — see `crates/gui/src/lib/dbus.ts`.

## Roadmap

- [x] openWakeWord pipeline (this release)
- [x] D-Bus mutation methods + comment-preserving TOML persist
- [x] horchd-gui Tauri tray + control panel
- [x] `horchctl wakeword import <url-or-path>` — one-shot fetch + register from any URL or local file
- [x] `ScoreSnapshot(name, score)` D-Bus signal at ~5 Hz so subscribers can render live meters without polling
- [ ] [micro-wake-word](https://github.com/OHF-Voice/micro-wake-word)
      backend behind an `engine = "openwakeword" | "microwakeword"`
      config field — same audio capture, different inference stack
      (TFLite micro models, different feature frontend)
- [ ] AUR submission
      so subscribers can render live meters without polling
- [ ] Custom domain at <https://horchd.xyz>

## Acknowledgements

This project stands on:

- [openWakeWord](https://github.com/dscripka/openWakeWord) — the
  inference pipeline (melspec + embedding + classifier shapes), bundled
  preprocessing models, and several pretrained wakeword classifiers.
- [`ort`](https://crates.io/crates/ort) — the official-ish Rust
  bindings to ONNX Runtime that make the inference path possible.
- [`cpal`](https://crates.io/crates/cpal) — cross-platform audio
  capture; on Linux it transparently speaks PipeWire / PulseAudio /
  ALSA.
- [`zbus`](https://crates.io/crates/zbus) — pure-Rust D-Bus library
  used for both the server-side interface and the client-side proxy.
- [Tauri 2](https://tauri.app) + [SvelteKit](https://kit.svelte.dev) +
  [Tailwind CSS v4](https://tailwindcss.com) +
  [Lucide](https://lucide.dev) — the GUI stack.
- [micro-wake-word](https://github.com/OHF-Voice/micro-wake-word) —
  the inspiration for the planned dual-engine architecture, and the
  wakeword engine ESPHome / Home Assistant Voice ship today.

## Contributing

Issues + PRs welcome at <https://codeberg.org/NewtTheWolf/horchd>. Run
`cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D
warnings` before opening one. The full build plan + design notes live
at <https://docs.horchd.xyz>.

## License

Dual `MIT OR Apache-2.0`, your choice. See
[`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE).
