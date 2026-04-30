#!/usr/bin/env bash
# Generate an HTML test-coverage report for the workspace via
# `cargo-llvm-cov`.  Output lands in `target/llvm-cov/html/index.html`;
# the script also prints a summary table to stdout.
#
# First run installs the helper if it isn't already on PATH.

set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/.."

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
  echo "→ installing cargo-llvm-cov (one-time, ~1 minute)…" >&2
  cargo install cargo-llvm-cov --locked
fi

# Clean stale profraw files so a re-run reports the freshly-built tests.
cargo llvm-cov clean --workspace

# `--workspace --all-targets` covers lib + bin + bench harness; we drop
# the actual benchmark functions (which never run under llvm-cov) via
# the default Criterion no-harness skip.
cargo llvm-cov --workspace --all-targets --html "$@"

cargo llvm-cov --workspace --all-targets --summary-only

echo
echo "→ HTML report: target/llvm-cov/html/index.html"
echo "→ open with:   xdg-open target/llvm-cov/html/index.html"
