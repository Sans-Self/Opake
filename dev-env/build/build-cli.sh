#!/usr/bin/env bash
# Build (and freshness-check) the opake CLI image the dev-env bootstrap and the
# federation test tier run. The compiled binary is baked into the image, so a
# stale image silently certifies an old binary against current source — exactly
# the fake-pds class of failure. To make that impossible, the build stamps a
# content hash of the CLI's build inputs as the `opake.src_hash` image label,
# and `ensure` rebuilds whenever the working tree no longer matches the label.
#
# Single source of truth for building this image: build-images.sh and the
# justfile `_dev-env-cli-fresh` gate both go through here.
#
#   build-cli.sh hash     print the source hash of the working tree
#   build-cli.sh build     build the image, baking the hash as a label
#   build-cli.sh ensure    rebuild iff the baked label != the working-tree hash
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
IMAGE="opake-devenv-cli:pinned"
LABEL_KEY="opake.src_hash"

# Deterministic content hash over exactly the CLI's build inputs. Paths are
# repo-relative (we cd first) and sorted so the digest is stable across runs
# and machines; the target dir is never an input.
src_hash() {
  cd "$REPO"
  {
    shasum Cargo.toml Cargo.lock
    find crates apps/cli -type f -not -path '*/target/*' -print0 \
      | sort -z | xargs -0 shasum
  } | shasum | cut -d' ' -f1
}

image_hash() {
  docker image inspect "$IMAGE" \
    --format "{{ index .Config.Labels \"$LABEL_KEY\" }}" 2>/dev/null || true
}

build() {
  local hash="$1"
  local ctx
  ctx="$(mktemp -d)"
  cp "$REPO/Cargo.toml" "$REPO/Cargo.lock" "$ctx/"
  cp -R "$REPO/crates" "$ctx/crates"
  mkdir -p "$ctx/apps" && cp -R "$REPO/apps/cli" "$ctx/apps/cli"
  find "$ctx" -type d -name target -prune -exec rm -rf {} + 2>/dev/null || true
  docker build -t "$IMAGE" --label "$LABEL_KEY=$hash" -f "$HERE/cli.Dockerfile" "$ctx"
  rm -rf "$ctx"
}

case "${1:-build}" in
  hash)
    src_hash
    ;;
  build)
    hash="$(src_hash)"
    build "$hash"
    echo "built $IMAGE (src_hash=$hash)"
    ;;
  ensure)
    want="$(src_hash)"
    have="$(image_hash)"
    if [ "$want" = "$have" ]; then
      echo "dev-env CLI image fresh (src_hash=$want)"
    else
      echo "dev-env CLI image stale (source=$want, image=${have:-none}) — rebuilding"
      build "$want"
      echo "rebuilt $IMAGE (src_hash=$want)"
    fi
    ;;
  *)
    echo "usage: build-cli.sh [hash|build|ensure]" >&2
    exit 2
    ;;
esac
