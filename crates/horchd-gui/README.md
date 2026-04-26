# horchd-gui

Tauri 2 tray + control panel for the [horchd](../../) wakeword
detection daemon. Talks to the daemon entirely over D-Bus on the
session bus — no extra IPC, no shared state.

The frontend is a single hand-rolled `src-web/index.html` (HTML + CSS +
vanilla JS) committed to the **brutalist scientific instrument**
aesthetic — warm parchment background, ink-black text, one signal-red
accent for fires, **Fraunces** italic display serif for hero numerals
and the wordmark, **IBM Plex Mono** for everything else, and 1px
hairlines instead of rounded corners.

The plan calls for an eventual SvelteKit + Tailwind v4 + `@lucide/svelte`
migration; this scaffold serves as the visual + behavioural reference
for that port.

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
- `icons/tray.png` (small monochrome PNG for the tray bar)

### 5. Run it

```bash
cd crates/horchd-gui
cargo tauri dev      # dev with hot-reload (uses src-web/index.html as the static frontend)
cargo tauri build    # release bundle (.deb / .rpm / .AppImage)
```

The `src-web/index.html` page also runs **standalone in a regular
browser** with mocked daemon data, which is handy for tweaking the
design without rebuilding Tauri:

```bash
cd crates/horchd-gui/src-web
python -m http.server 8000   # or any static server
xdg-open http://localhost:8000
```

The mocked branch kicks in whenever `window.__TAURI__` is undefined.

## Frontend architecture

`src-web/index.html` is a single self-contained file:

- **Tokens** — light & dark themes via `prefers-color-scheme`, all
  colours/typography/spacing as CSS variables.
- **Live data path** — `setInterval(pollStatus, 1000)` calls `get_status`
  for the FPS gauges; `setInterval(loadWakes, 5000)` reconciles the
  wakeword card list. Detected fires arrive over the
  `horchd://detected` Tauri event (emitted by `src/events.rs`) and flash
  the matching card + push to the recent-fires ticker.
- **Threshold sliders** — drag fires `set_threshold(..., save=false)`
  debounced 220 ms. The dashed "Save" button next to each slider
  becomes solid red when dirty; clicking persists with
  `set_threshold(..., save=true)`.
- **Add modal** — vanilla `<form>` + Esc-to-close + scrim click. Submits
  to `add_wakeword`.

## D-Bus surface used

Every Tauri command in `src/commands.rs` is a thin wrapper over a
single `xyz.horchd.Daemon1` call:

| Tauri command                        | D-Bus method     |
| ------------------------------------ | ---------------- |
| `list_wakewords()`                   | `ListWakewords`  |
| `get_status()`                       | `GetStatus`      |
| `set_threshold(name, value, save)`   | `SetThreshold`   |
| `set_enabled(name, enabled, save)`   | `SetEnabled`     |
| `set_cooldown(name, ms, save)`       | `SetCooldown`    |
| `add_wakeword(name, model, …)`       | `Add`            |
| `remove_wakeword(name)`              | `Remove`         |
| `reload()`                           | `Reload`         |

`src/events.rs` runs a long-lived task that subscribes to the
`Detected` D-Bus signal and rebroadcasts it as a Tauri frontend event;
the loop reconnects automatically if the daemon restarts.
