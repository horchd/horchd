---
eleventyNavigation:
  key: install
  title: Install
  parent: getting-started
  order: 10
description: "Install horchd from source, AUR, or via cargo. Bundled vs. dynamic ONNX Runtime trade-offs and systemd user-unit setup."
---

## Source install (any distro with cargo)

```bash
git clone https://codeberg.org/NewtTheWolf/horchd
cd horchd

# Drop the universal preprocessing models from openWakeWord
oww=$(python -c 'import openwakeword, pathlib; print(pathlib.Path(openwakeword.__file__).parent / "resources/models")')
cp "$oww"/{melspectrogram,embedding_model}.onnx shared-models/

./packaging/install.sh
```

`install.sh` builds release binaries, installs them to `/usr/local/bin/`,
copies the shared models to `/usr/local/share/horchd/`, scaffolds
`~/.config/horchd/config.toml` and the user data directory
`~/.local/share/horchd/models/`, then enables and starts the systemd user
unit.

Verify:

```bash
systemctl --user status horchd
horchctl status
```

## ONNX Runtime: bundled vs dynamic

`horchd` ships with two cargo features that control how the ONNX Runtime
shared library is resolved:

| Feature                  | Default | Binary footprint     | Setup                                                              |
| ------------------------ | :-----: | -------------------- | ------------------------------------------------------------------ |
| `bundled-onnxruntime`    | yes     | ~29 MB statically    | Nothing — `ort` downloads + bundles the matching ONNX Runtime.     |
| `dynamic-onnxruntime`    |         | ~5 MB binary alone   | You install ONNX Runtime separately and `LD_LIBRARY_PATH` it in.   |

Pick `bundled` for a self-contained binary you can `scp` anywhere, and
`dynamic` if your distro already ships ONNX Runtime (Arch's `onnxruntime`
package, Debian's `libonnxruntime`, etc.) and you'd rather trim the daemon
down.

```bash
# bundled (default — single self-contained binary)
cargo install --git https://codeberg.org/NewtTheWolf/horchd horchd

# dynamic (smaller binary, requires onnxruntime on the system)
cargo install --git https://codeberg.org/NewtTheWolf/horchd horchd \
  --no-default-features --features dynamic-onnxruntime
```

## Arch / CachyOS (AUR)

A `PKGBUILD` ships under `packaging/arch/`. Once published to the AUR:

```bash
yay -S horchd                   # or paru / makepkg
```

The AUR package depends on `pipewire`. After install, fetch the shared
models the same way as the source path (the package leaves them up to
the user to keep the build hermetic):

```bash
sudo install -Dm644 "$oww/melspectrogram.onnx"  /usr/share/horchd/melspectrogram.onnx
sudo install -Dm644 "$oww/embedding_model.onnx" /usr/share/horchd/embedding_model.onnx
```

Then enable the unit and reload the config:

```bash
systemctl --user enable --now horchd
horchctl reload
```

## Build dependencies

If you build from source you need:

- `rustc` 1.85+ (Rust 2024 edition)
- `cargo`
- A working PipeWire (or PulseAudio) install — cpal handles both
- libc, glibc — already on every Linux desktop

`ort` will download a matching ONNX Runtime binary on first build (with the
`download-binaries` feature, which is on by default for the bundled variant).
