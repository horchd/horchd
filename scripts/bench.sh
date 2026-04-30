#!/usr/bin/env bash
# Run the Criterion benchmarks. Reports land under `target/criterion/`
# with HTML index at `target/criterion/report/index.html`.

set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/.."

cargo bench --workspace "$@"

echo
echo "→ HTML report: target/criterion/report/index.html"
