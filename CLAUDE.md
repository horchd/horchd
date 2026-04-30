# CLAUDE.md — instructions for Claude Code working in this repo

This file is the **always-loaded** context. Read it at the start of every
session. The companion `plan.md` (untracked, local-only) holds the long-form
project blueprint; consult it when you need to recover context but don't
duplicate its contents here.

## Local folder layout (filesystem, not git)

```
/home/newt/Dokumente/Projects/Personal/horchd/    ← org folder (mirrors github.com/horchd/)
├── horchd/                                        ← THIS repo (the daemon workspace)
└── horchd.github.io/                              ← Phase-10 docs site repo (future)
```

The outer `horchd/` is a local convenience for grouping all `horchd/*` GitHub
repos, not a git repo itself. Always `cd` into `horchd/horchd/` before
running cargo or git commands for the daemon.

---

## What this project is

`horchd` is a native Linux daemon that loads N user-defined wakeword models
in parallel, listens to the system microphone, and broadcasts a D-Bus signal
the moment any of them fires. Subscribers (scripts, Home Assistant, the
`horchctl` CLI, the `horchd-gui` Tauri app) react to those signals.

Cargo workspace monorepo with these crates:

| Crate | Kind | Purpose |
| --- | --- | --- |
| `horchd-client` | lib | Shared types: config schema, D-Bus interface trait, event struct, error type. Zero runtime dependencies on the others. |
| `horchd` | bin | The daemon. Owns the audio capture + ONNX inference + D-Bus service. |
| `horchctl` | bin | CLI client (think `systemctl`). Talks to the daemon over D-Bus. |
| `horchd-gui` | bin | (Phase 9) Tauri tray + control panel. Talks to the daemon over D-Bus. |

## Locked-in decisions (don't re-litigate without asking)

| | |
| --- | --- |
| Project name | `horchd` |
| GitHub org/repo | `github.com/horchd/horchd` |
| Domain | `horchd.xyz` |
| License | `MIT OR Apache-2.0` (both LICENSE files at repo root) |
| Rust edition | `2024` |
| Models directory | `~/.local/share/horchd/models/` only — no system-wide path, no auto-discovery |
| Service type | systemd **user** unit |
| D-Bus bus | session bus |
| D-Bus service name | `xyz.horchd.Daemon` |
| D-Bus object path | `/xyz/horchd/Daemon` |
| D-Bus interface | `xyz.horchd.Daemon1` |
| `GetStatus` return tuple | `(bddd)` — running, audio_fps, score_fps, mic_level |
| Initial commit message | `Batman` (no body, no trailers — by user request) |

## Reference material outside this repo

The openWakeWord Python implementation is the spec we are porting. The
upstream source is at <https://github.com/dscripka/openWakeWord>; the
files to consult when verifying inference behavior are
`openwakeword/model.py` and `openwakeword/utils.py`.

The universal preprocessing models (`melspectrogram.onnx`,
`embedding_model.onnx`) ship inside the upstream package under
`openwakeword/resources/models/`. `install.sh` copies them into
`/usr/local/share/horchd/`; for development, drop them into
`shared-models/` (gitignored) — see the `oww=$(python -c ...)` snippet
in the README.

## Pipeline shape (memorize this)

```
cpal mic 16 kHz mono
  → 80 ms / 1280-sample frames
  → melspectrogram.onnx                  (universal)
  → embedding_model.onnx                 (universal, 96-dim per 80 ms)
  → sliding window of last 16 embeddings (1.28 s receptive field)
  → fan-out to per-wakeword classifier   (input (1, 16, 96), output f32 in [0,1])
  → threshold + cooldown state machine
  → D-Bus Detected(name, score, timestamp_us) signal
```

## Where to look first

- `plan.md` — full implementation roadmap, phase checklists, all spec details (TOML format, D-Bus methods/signals, install script, Tauri GUI plan, GH Pages docs site plan). **Not committed.**
- `Cargo.toml` (root) — workspace + pinned `[workspace.package]` metadata.
- `crates/*/Cargo.toml` — per-crate deps, all pinned via `cargo add`.
- `crates/*/src/{main.rs,lib.rs}` — currently placeholder stubs.

---

## Workflow Orchestration

### 1. Plan Node Default

- Enter plan mode for ANY non-trivial task (3+ steps or architectural decisions).
- If something goes sideways, STOP and re-plan immediately — don't keep pushing.
- Use plan mode for verification steps, not just building.
- Write detailed specs upfront to reduce ambiguity.

### 2. Subagent Strategy

- Use subagents liberally to keep main context window clean.
- Offload research, exploration, and parallel analysis to subagents.
- For complex problems, throw more compute at it via subagents.
- One **task** per subagent for focused execution.

### 3. Self-Improvement Loop

- After ANY correction from the user: update `tasks/lessons.md` with the pattern.
- Write rules for yourself that prevent the same mistake.
- Ruthlessly iterate on these lessons until mistake rate drops.
- Review lessons at session start for relevant project.

### 4. Verification Before Done

- Never mark a task complete without proving it works.
- Diff behavior between main and your changes when relevant.
- Ask yourself: "Would a staff engineer approve this?"
- Run tests, check logs, demonstrate correctness.

### 5. Demand Elegance (Balanced)

- For non-trivial changes: pause and ask "is there a more elegant way?"
- If a fix feels hacky: "Knowing everything I know now, implement the elegant solution."
- Skip this for simple, obvious fixes — don't over-engineer.
- Challenge your own work before presenting it.

### 6. Autonomous Bug Fixing

- When given a bug report: just fix it. Don't ask for hand-holding.
- Point at logs, errors, failing tests — then resolve them.
- Zero context switching required from the user.
- Go fix failing CI tests without being told how.

---

## Task Management

1. **Plan First**: Write plan to `tasks/todo.md` with checkable items.
2. **Verify Plan**: Check in before starting implementation.
3. **Track Progress**: Mark items complete as you go.
4. **Explain Changes**: High-level summary at each step.
5. **Document Results**: Add review section to `tasks/todo.md`.
6. **Capture Lessons**: Update `tasks/lessons.md` after corrections.

`tasks/` is git-ignored — these are personal scratch files for the agent,
not project artifacts. Create them on demand; don't commit them.

---

## Core Principles

- **Simplicity First**: Make every change as simple as possible. Impact minimal code.
- **No Laziness**: Find root causes. No temporary fixes. Senior developer standards.
- **Minimal Impact**: Changes should only touch what's necessary. Avoid introducing bugs.

---

## Repo conventions

- Format: `cargo fmt --all` before any commit.
- Lint: `cargo clippy --workspace --all-targets -- -D warnings` must pass.
- Build: `cargo check` must succeed at every commit boundary; `cargo build --release` before tagging.
- Commit messages: short, imperative, no AI-tool trailers. The very first commit is literally `Batman`.
- Don't add comments that just restate the code. Explain *why*, never *what*.
- Don't write documentation files unless they're in `plan.md`'s explicit checklist or the user asked for them.
- Don't create new top-level files without a clear reason.
