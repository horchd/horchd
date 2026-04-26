#!/usr/bin/env bash
# horchd installer — builds release binaries, installs them under PREFIX
# (default /usr/local), copies the bundled openWakeWord shared models,
# scaffolds the user config + data dirs, and enables the systemd user unit.
#
# Re-runnable: existing config files are left untouched; only the binaries
# and the systemd unit are overwritten.
set -euo pipefail

PREFIX="${PREFIX:-/usr/local}"
USER_DATA="${XDG_DATA_HOME:-$HOME/.local/share}/horchd"
USER_CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}/horchd"
USER_SYSTEMD="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "==> building release binaries"
cargo build --release --bin horchd --bin horchctl

echo "==> installing binaries to $PREFIX/bin (requires sudo)"
sudo install -Dm755 target/release/horchd  "$PREFIX/bin/horchd"
sudo install -Dm755 target/release/horchctl "$PREFIX/bin/horchctl"

echo "==> installing shared ONNX models to $PREFIX/share/horchd"
if [[ ! -f shared-models/melspectrogram.onnx ]] || [[ ! -f shared-models/embedding_model.onnx ]]; then
  cat <<'MSG' >&2
shared-models/melspectrogram.onnx and/or embedding_model.onnx are missing.
Copy them from your openwakeword install, e.g.:

  cp <openwakeword-venv>/lib/python*/site-packages/openwakeword/resources/models/{melspectrogram,embedding_model}.onnx shared-models/

then re-run this installer.
MSG
  exit 1
fi
sudo install -Dm644 shared-models/melspectrogram.onnx  "$PREFIX/share/horchd/melspectrogram.onnx"
sudo install -Dm644 shared-models/embedding_model.onnx "$PREFIX/share/horchd/embedding_model.onnx"

echo "==> ensuring user config + data dirs exist"
mkdir -p "$USER_DATA/models" "$USER_CONFIG"
if [[ ! -f "$USER_CONFIG/config.toml" ]]; then
  cp examples/horchd.toml "$USER_CONFIG/config.toml"
  echo "    seeded $USER_CONFIG/config.toml from examples/horchd.toml"
else
  echo "    keeping existing $USER_CONFIG/config.toml"
fi

echo "==> installing systemd user unit"
mkdir -p "$USER_SYSTEMD"
install -Dm644 systemd/horchd.service "$USER_SYSTEMD/horchd.service"
systemctl --user daemon-reload

echo "==> enabling + starting horchd.service"
systemctl --user enable --now horchd.service

echo
systemctl --user status horchd --no-pager || true
echo
echo "Done. Edit $USER_CONFIG/config.toml to register wakeword models,"
echo "then run:  horchctl reload"
