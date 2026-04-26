# horchd-gui

Tauri 2 tray app + control panel for the [horchd](../../) wakeword
detection daemon. Talks to the daemon entirely over D-Bus on the
session bus — no extra IPC, no shared state.

The frontend is a SvelteKit (Svelte 5 runes) app styled with Tailwind
v4 and `@lucide/svelte` icons, committed to the **brutalist scientific
instrument** aesthetic — warm parchment background, ink-black text,
one signal-red accent for fires, **Fraunces** italic display serif for
hero numerals and the wordmark, **IBM Plex Mono** for everything else,
and 1 px hairlines instead of rounded corners.

## Layout

This crate follows the canonical [Tauri 2 + SvelteKit][tauri-svelte]
project layout:

```
crates/horchd-gui/
├── package.json            ← frontend deps + scripts (bun)
├── svelte.config.js
├── vite.config.ts
├── tsconfig.json
├── src/                    ← SvelteKit frontend
│   ├── app.html, app.css
│   ├── lib/{components, dbus.ts, app.svelte.ts, …}
│   └── routes/+page.svelte
├── static/                 ← passthrough assets
└── src-tauri/              ← Rust + Tauri (workspace member)
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── build.rs
    ├── capabilities/default.json
    ├── icons/
    └── src/{main.rs, lib.rs, commands.rs, dbus_client.rs, events.rs, tray.rs}
```

[tauri-svelte]: https://v2.tauri.app/start/frontend/sveltekit/

## Bootstrap

### 1. System dependencies (Linux)

```bash
# Arch / CachyOS
sudo pacman -S webkit2gtk-4.1 librsvg libayatana-appindicator xdotool

# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev \
                 libayatana-appindicator3-dev libxdo-dev
```

### 2. Tauri CLI (one-time)

```bash
cargo install tauri-cli --version "^2" --locked
# OR via the bun-managed copy that ships in package.json:
cd crates/horchd-gui && bun install
```

Either route gives you the `tauri` command — system-wide via cargo, or
project-local via `bun run tauri …`.

### 3. Run

```bash
cd crates/horchd-gui

# Production-style: build static frontend + native bundle
cargo tauri build

# Dev: vite dev server + Tauri shell with HMR (auto-spawns both)
cargo tauri dev

# Lower-level: just `cargo run` against the prebuilt frontend
bun run build && cargo run -p horchd-gui --release
```

`cargo tauri dev` honours `tauri.conf.json` `beforeDevCommand: "bun run dev"`
— vite starts on `localhost:5173`, Tauri webview points at it, edits
hot-reload. `cargo tauri build` runs `beforeBuildCommand: "bun install
--frozen-lockfile && bun run build"` first, then bakes the static
output into the Tauri bundle.

### 4. Browser-only design preview

The SvelteKit page **runs standalone in any browser** with mocked
daemon data, which is great for tweaking the design without a daemon
or rebuilding the Tauri shell:

```bash
cd crates/horchd-gui
bun run dev
xdg-open http://localhost:5173
```

The mock branch in `src/lib/dbus.ts` kicks in whenever
`window.__TAURI_INTERNALS__` is undefined.

## Frontend architecture

| File | Role |
| --- | --- |
| `src/app.css` | Tailwind v4 `@theme` tokens (colors, fonts) + light/dark via `prefers-color-scheme` |
| `src/lib/types.ts` | Shared TS types (DaemonStatus, WakewordRow, ScorePayload, …) |
| `src/lib/dbus.ts` | Typed wrappers around Tauri commands; mock backend for browser-preview mode |
| `src/lib/app.svelte.ts` | Singleton `app` store using Svelte 5 runes; owns polling timers, event subscribers, toast queue |
| `src/lib/components/*.svelte` | `Header`, `Gauge`, `Spark`, `WakeCard`, `AddModal`, `Toast`, `Ticker`, `StatusPill` |
| `src/routes/+layout.{ts,svelte}` | `prerender = true; ssr = false;` — adapter-static needs SPA mode |
| `src/routes/+page.svelte` | The main control panel layout |

## D-Bus surface

Every Tauri command in `src-tauri/src/commands.rs` is a thin wrapper
over a single `xyz.horchd.Daemon1` call:

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

`src-tauri/src/events.rs` runs two long-lived tasks that subscribe to
the daemon's `Detected` and `ScoreSnapshot` D-Bus signals and
rebroadcast them as Tauri frontend events on `horchd://detected` and
`horchd://score` respectively. Both loops reconnect automatically if
the daemon restarts.
