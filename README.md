# horchd

> A native Linux daemon that listens to the system microphone, detects
> any of *N* user-defined wakewords in parallel, and broadcasts a D-Bus
> `Detected` signal the moment one fires.

[![ci](https://github.com/horchd/horchd/actions/workflows/ci.yml/badge.svg)](https://github.com/horchd/horchd/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![docs](https://img.shields.io/badge/docs-horchd.github.io-c8311c)](https://horchd.github.io)

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
you want a native daemon instead of a Python process, and a companion
to [Lyna](https://github.com/horchd/lyna) which trains the `.onnx`
classifiers `horchd` loads.

---

## Highlights

- **Native** — single Rust 2024 binary; ONNX Runtime as the only big dep
- **Multi-wakeword** — N classifiers run on the same audio frame, fan-out at the embedding stage
- **Cheap** — ~12.5 inferences/sec on a single core, ~1.5 MB shared models + ~80 KB per wakeword
- **D-Bus first** — no HTTP listener, no custom socket, no cloud
- **systemd user unit** — no root, no system-bus policy file
- **Hot-reload** — edit the TOML, `horchctl reload`, never drops the audio thread
- **Trainer-agnostic** — bring `.onnx` from [Lyna](https://github.com/horchd/lyna) or any
  [openWakeWord](https://github.com/dscripka/openWakeWord)-compatible trainer
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
lyna  jarvis wetter …    per-wakeword .onnx classifier  →  score in [0,1]
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

## Repository layout

```
horchd/
├── crates/
│   ├── horchd-core/   shared types + D-Bus proxy trait (consumed by every binary)
│   ├── horchd/        the daemon
│   ├── horchctl/      CLI client (status, list, monitor, threshold, add, remove, reload, …)
│   └── horchd-gui/    Tauri 2 tray + control panel (SvelteKit + Tailwind v4)
├── shared-models/     melspectrogram.onnx + embedding_model.onnx (gitignored)
├── systemd/           user unit
├── packaging/         install.sh + arch/PKGBUILD
├── examples/          horchd.toml + subscriber.{sh,py}
└── .github/workflows/ ci.yml (fmt + clippy + test + frontend) + release.yml
```

## Install

### From source

```bash
git clone https://github.com/horchd/horchd
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

### Docker / Nix

Out of scope for v0.1 — the daemon needs raw mic access and a real
session bus, both of which fight container isolation. PRs welcome.

## First wakeword (60-second walkthrough)

```bash
# Pull a pretrained openWakeWord model into the user models dir
mkdir -p ~/.local/share/horchd/models
oww=$(python -c 'import openwakeword, pathlib; print(pathlib.Path(openwakeword.__file__).parent / "resources/models")')
cp "$oww/hey_jarvis_v0.1.onnx" ~/.local/share/horchd/models/

# Register it with the daemon (validates shape, loads it, persists to TOML)
horchctl add hey_jarvis --model ~/.local/share/horchd/models/hey_jarvis_v0.1.onnx

# Verify
horchctl status
horchctl monitor      # speak "hey jarvis"
```

For a custom wakeword, train one in [Lyna](https://github.com/horchd/lyna),
drop the resulting `<name>.onnx` into `~/.local/share/horchd/models/`,
and `horchctl add <name> --model …`.

## horchctl

```bash
horchctl status                                   # daemon health + loaded wakewords
horchctl list                                     # tabular view
horchctl monitor                                  # tail Detected signals live

horchctl threshold lyna 0.45                      # transient (resets on restart)
horchctl threshold lyna 0.45 --save               # persist to config.toml (preserves comments)
horchctl cooldown  lyna 1200 --save
horchctl enable    lyna --save
horchctl disable   lyna --save

horchctl add wetter --model ~/.local/share/horchd/models/wetter.onnx --threshold 0.55
horchctl remove wetter            # keeps the .onnx on disk
horchctl remove wetter --purge    # also deletes the .onnx + .onnx.data sibling

horchctl reload                   # re-read config.toml; hot-keep unchanged models
```

All mutator commands either error out cleanly (validates shape /
unique name / cooldown) or echo back what they did. `--save` writes
through [`toml_edit`](https://crates.io/crates/toml_edit) so user
comments and ordering survive.

## horchd-gui

A Tauri 2 tray app + control panel for users who'd rather not live in
the terminal. Talks to the daemon over the same D-Bus surface as
`horchctl`, no special privileges.

Stack: **SvelteKit** (Svelte 5 runes) + **Tailwind v4** + **@lucide/svelte** +
**Bun**. Design language: brutalist scientific instrument — warm
parchment background, ink-black text, one signal-red accent for fires,
**Fraunces** italic serif for hero numerals and the wordmark, **IBM
Plex Mono** for everything else, hairline borders instead of rounded
corners.

```bash
cd crates/horchd-gui/src-web
bun install
bun run build         # outputs to src-web/build/

cd ..
cargo run --release   # opens the tray + window; daemon must be running
```

See [crates/horchd-gui/README.md](crates/horchd-gui/README.md) for the
full bootstrap (Linux dev headers + Tauri CLI for `cargo tauri build`
to produce `.deb`/`.rpm`/`.AppImage`).

The Wayland workaround for `Gdk Error 71` (NVIDIA + webkit2gtk +
Wayland) is set automatically inside the binary; nothing to configure.

## D-Bus API

Bus: **session bus**. No system-bus policy, runs as the user.

```
Service:    xyz.horchd.Daemon
Object:     /xyz/horchd/Daemon
Interface:  xyz.horchd.Daemon1
```

| Method          | Args                                                | Returns        |
| --------------- | --------------------------------------------------- | -------------- |
| `ListWakewords` | —                                                   | `a(sdsbu)`     |
| `GetStatus`     | —                                                   | `(bdd)`        |
| `Add`           | `s name`, `s model_path`, `d threshold`, `u cooldown_ms` | `()`      |
| `Remove`        | `s name`                                            | `()`           |
| `SetThreshold`  | `s name`, `d threshold`, `b persist`                | `()`           |
| `SetEnabled`    | `s name`, `b enabled`, `b persist`                  | `()`           |
| `SetCooldown`   | `s name`, `u ms`, `b persist`                       | `()`           |
| `Reload`        | —                                                   | `()`           |

| Signal     | Args                                  |
| ---------- | ------------------------------------- |
| `Detected` | `s name`, `d score`, `t timestamp_us` |

Full reference + introspection output: <https://horchd.github.io/dbus-api>.

## Subscribers

```bash
busctl --user monitor xyz.horchd.Daemon
gdbus monitor --session --dest xyz.horchd.Daemon --object-path /xyz/horchd/Daemon
```

Bash, Python (`dbus-next`), and Rust (`zbus` + `horchd-core`) full
examples live under [`examples/`](examples/) and at
<https://horchd.github.io/examples/bash> /
[`/python`](https://horchd.github.io/examples/python) /
[`/rust`](https://horchd.github.io/examples/rust).

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
name = "lyna"                # appears in the D-Bus signal
model = "~/.local/share/horchd/models/lyna.onnx"
threshold = 0.5              # default
cooldown_ms = 1500           # default
enabled = true               # default
```

Field reference: <https://horchd.github.io/config>.

## Develop

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUST_LOG=horchd=debug cargo run --bin horchd -- --config examples/horchd.toml
```

GUI dev:

```bash
cd crates/horchd-gui/src-web
bun install
bun run dev          # vite dev server with HMR
# In another terminal: cargo run -p horchd-gui  (Tauri loads http://localhost:5173)
```

The SvelteKit page also runs **standalone in any browser** with
deterministic mock data, so you can iterate on the design without a
running daemon:

```bash
cd crates/horchd-gui/src-web
bun run dev
xdg-open http://localhost:5173
```

CI runs fmt + clippy + workspace tests + frontend type-check + frontend
build on every push and PR. Tagged releases (`v*.*.*`) build a tarball
of the daemon + CLI + systemd unit + examples and publish it to a
GitHub Release.

## Roadmap

- [x] openWakeWord pipeline (this release)
- [x] D-Bus mutation methods + comment-preserving TOML persist
- [x] horchd-gui Tauri tray + control panel
- [ ] [micro-wake-word](https://github.com/OHF-Voice/micro-wake-word)
      backend behind an `engine = "openwakeword" | "microwakeword"`
      config field — same audio capture, different inference stack
      (TFLite micro models, different feature frontend)
- [ ] AUR submission
- [ ] `horchctl import-pretrained <name>` — one-shot fetch of an
      upstream openWakeWord model into the user models dir
- [ ] Optional `ScoreSnapshot(name, score)` D-Bus signal at low rate
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
- [Lyna](https://github.com/horchd/lyna) — the companion trainer/studio.

## Contributing

Issues + PRs welcome at <https://github.com/horchd/horchd>. CI must
stay green; see `.github/workflows/ci.yml` for the gates. The full
build plan + design notes live at <https://horchd.github.io>.

## License

Dual `MIT OR Apache-2.0`, your choice. See
[`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE).
