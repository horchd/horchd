---
eleventyNavigation:
  title: Overview
permalink: /
description: "horchd is a Rust daemon that loads N user-defined ONNX wakeword classifiers in parallel and broadcasts a D-Bus Detected signal to any subscriber on the Linux session bus."
---

`horchd` is a native multi-wakeword detection daemon written in Rust. It loads
N user-defined ONNX classifiers in parallel, listens to the system microphone,
and broadcasts a typed D-Bus signal the moment any wakeword fires. The audio
thread runs once, the two universal preprocessing models load once, and adding
the N-th wakeword costs one classifier evaluation per 80 ms input frame.

These docs cover the daemon, the `horchctl` CLI, the configuration schema, the
D-Bus contract, and ready-to-paste subscriber snippets in Bash, Python, and
Rust.

## Where to start

- **[Install](/getting-started/install/)** — source install, AUR, or one-liner
  `cargo install`. Two ort variants (bundled vs dynamic) for the binary
  footprint trade-off.
- **[Quickstart](/getting-started/quickstart/)** — register your first
  wakeword and verify the daemon fires.
- **[Configuration](/reference/configuration/)** — TOML schema, defaults,
  and how `horchctl … --save` edits the file in place without dropping
  comments.
- **[D-Bus API](/reference/dbus-api/)** — methods, signals, signatures,
  `busctl` examples — the canonical contract.

## Subscribe from your stack

| Language | Recipe                                                  |
| -------- | ------------------------------------------------------- |
| Bash     | [`gdbus monitor` snippet](/recipes/bash-subscriber/)    |
| Python   | [`dbus-next` and `jeepney`](/recipes/python-subscriber/) |
| Rust     | [`zbus` + `horchd-client`](/recipes/rust-subscriber/)   |

## License

Dual `MIT OR Apache-2.0` — the standard Rust ecosystem dual-license. Either
is yours to choose. Source-first on
[Codeberg](https://codeberg.org/NewtTheWolf/horchd); the
[GitHub repository](https://github.com/horchd/horchd) is a read-only mirror.
