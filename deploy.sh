#!/usr/bin/env bash
# Build the static site and force-push the contents of dist/ to the `pages`
# branch on origin (Codeberg). Source lives on `main`.
#
# Idempotent. Run from this directory.
set -euo pipefail

cd "$(dirname "$0")"

echo "→ build"
bun run build

cd dist

echo "→ stage pages branch from dist/"
rm -rf .git
git init -q -b pages
git add .
git -c user.email=deploy@horchd.xyz -c user.name=deploy commit -q -m "deploy $(date -u +%Y-%m-%dT%H:%M:%SZ)"
git remote add origin "$(cd .. && git config --get remote.origin.url)"
git push -f origin pages

echo "→ done · https://horchd.xyz (propagation may take a minute)"
