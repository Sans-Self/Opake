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
# Single source of truth for the CLI image (bakes the src-hash freshness label).
"$HERE/build-cli.sh" build

echo "== all images built =="
