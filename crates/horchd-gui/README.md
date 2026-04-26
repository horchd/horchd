# horchd-gui

Tauri 2 tray + control panel for the [horchd](../../) wakeword
detection daemon. Talks to the daemon entirely over D-Bus on the
session bus — no extra IPC, no shared state.

## Status

**Scaffolded only.** The crate is intentionally NOT in the workspace
`members` list yet, so `cargo build` from the repo root won't try to
build it. To build, follow the bootstrap below.

## Bootstrap

### 1. System dependencies (Linux)

```bash
# Arch / CachyOS
sudo pacman -S webkit2gtk-4.1 librsvg libayatana-appindicator xdotool

# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev \
                 libayatana-appindicator3-dev libxdo-dev
```

### 2. Tauri CLI

```bash
cargo install tauri-cli --version "^2" --locked
```

### 3. Add the crate to the workspace

In the repo root `Cargo.toml`:

```toml
[workspace]
members = [
  "crates/horchctl",
  "crates/horchd",
  "crates/horchd-core",
  "crates/horchd-gui",          # <-- add this line
]
```

### 4. Drop in icons

Tauri needs an icon set. Generate placeholders:

```bash
cd crates/horchd-gui
cargo tauri icon path/to/source-1024.png   # writes icons/*.png
```

Or hand-create:
- `icons/32x32.png`
- `icons/128x128.png`
- `icons/icon.png`
- `icons/tray.png` (tray bar icon — a small monochrome PNG)

### 5. Run it

```bash
cd crates/horchd-gui
cargo tauri dev      # dev with hot-reload (uses src-web/index.html as static frontend)
cargo tauri build    # release bundle (.deb / .rpm / .AppImage)
```

## Frontend

The current `src-web/index.html` is a vanilla HTML/JS placeholder that
exercises every Tauri command. The plan calls for migrating to
SvelteKit + Svelte 5 runes + Tailwind v4 + `@lucide/svelte` to match
the [Lyna](https://github.com/horchd/lyna) trainer's design language.
That migration is left as a separate PR — see plan.md Phase 9.

## D-Bus surface used

Every Tauri command in `src/commands.rs` is a thin wrapper over a
single `xyz.horchd.Daemon1` call (`ListWakewords`, `GetStatus`,
`SetThreshold`, `SetEnabled`, `SetCooldown`, `Add`, `Remove`,
`Reload`). The `Detected` signal is consumed by the (TODO) live event
log via Tauri events.
