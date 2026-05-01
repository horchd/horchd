#!/usr/bin/env bash
# Build the docs site and force-push the contents of _site/ to the
# `docs-pages` branch on origin (Codeberg) — origin is the daemon repo,
# so the built docs sit next to the source on a sibling branch. Source
# for this project lives on `docs-src`.
#
# Idempotent. Run from this directory.
set -euo pipefail

cd "$(dirname "$0")"

echo "→ build"
bun run build

cd _site

echo "→ stage docs-pages branch from _site/"
rm -rf .git
git init -q -b docs-pages
git add .
git -c user.email=dominik@spitzli.dev -c user.name=NewtTheWolf commit -q -m "deploy docs $(date -u +%Y-%m-%dT%H:%M:%SZ)"
git remote add origin "$(cd .. && git config --get remote.origin.url)"
git push -f origin docs-pages

echo "→ done · https://docs.horchd.xyz (propagation may take a minute)"
