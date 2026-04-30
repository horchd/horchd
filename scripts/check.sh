#!/usr/bin/env bash
# Pre-PR sanity check: fmt + clippy + test + frontend type-check + build.
# Mirrors the gates a CI workflow would enforce.

set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/.."

echo "→ cargo fmt --all -- --check"
cargo fmt --all -- --check

echo "→ cargo clippy --workspace --all-targets -- -D warnings"
cargo clippy --workspace --all-targets -- -D warnings

echo "→ cargo test --workspace"
cargo test --workspace

if [[ -d crates/horchd-gui/node_modules ]]; then
  echo "→ frontend type-check (svelte-check)"
  (cd crates/horchd-gui && bun run check)
else
  echo "→ frontend node_modules missing; skipping type-check (run \`cd crates/horchd-gui && bun install\`)"
fi

echo
echo "✓ all checks passed"
