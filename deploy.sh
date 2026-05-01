#!/usr/bin/env bash
# Build the static site and force-push the contents of dist/ to the `main`
# branch on origin (Codeberg). Source lives on `source` branch.
#
# Idempotent. Run from this directory.
set -euo pipefail

cd "$(dirname "$0")"

echo "→ build"
bun run build

cd dist

echo "→ stage main from dist/"
rm -rf .git
git init -q -b main
git add .
git -c user.email=deploy@horchd.xyz -c user.name=deploy commit -q -m "deploy $(date -u +%Y-%m-%dT%H:%M:%SZ)"
git remote add origin "$(cd .. && git config --get remote.origin.url)"
git push -f origin main

echo "→ done · https://horchd.xyz (propagation may take a minute)"
