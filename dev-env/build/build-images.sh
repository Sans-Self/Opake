#!/usr/bin/env bash
# Build every custom dev-env image with pinned tags. Idempotent; cache-warm
# rebuilds are fast. Called by `just dev-env-up` before `docker compose up`.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"

echo "== plc (did-method-plc, git context) =="
# Git build context; the Dockerfile lives inside that repo. (Its entrypoint
# reads DB_CREDS_JSON, not DATABASE_URL — see compose.)
docker build -t opake-devenv-plc:pinned \
  -f packages/server/Dockerfile \
  "https://github.com/did-method-plc/did-method-plc.git"

echo "== relay (indigo cmd/relay from source) =="
docker build -t opake-devenv-relay:pinned -f "$HERE/relay.Dockerfile" "$HERE"

echo "== jetstream (from source) =="
docker build -t opake-devenv-jetstream:pinned -f "$HERE/jetstream.Dockerfile" "$HERE"

echo "== indexer (mix release) =="
docker build -t opake-devenv-indexer:pinned -f "$HERE/indexer.Dockerfile" "$REPO/apps/indexer"

echo "== cli (opake, for bootstrap) =="
CLICTX="$(mktemp -d)"
cp "$REPO/Cargo.toml" "$REPO/Cargo.lock" "$CLICTX/"
cp -R "$REPO/crates" "$CLICTX/crates"
mkdir -p "$CLICTX/apps" && cp -R "$REPO/apps/cli" "$CLICTX/apps/cli"
find "$CLICTX" -type d -name target -prune -exec rm -rf {} + 2>/dev/null || true
docker build -t opake-devenv-cli:pinned -f "$HERE/cli.Dockerfile" "$CLICTX"
rm -rf "$CLICTX"

echo "== all images built =="
