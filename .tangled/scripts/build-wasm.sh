#!/usr/bin/env bash
set -euo pipefail

WASM_CRATE="crates/opake-wasm"
WASM_OUT="web/src/wasm/opake-wasm"
S3_BUCKET="sans-self-net"
WATCHED_PATHS="crates/opake-core crates/opake-wasm"

# Cache key = hash of the actual source content, not the commit
source_hash() {
  find $WATCHED_PATHS -type f -name '*.rs' -o -name 'Cargo.toml' | sort | xargs sha256sum | sha256sum | cut -d' ' -f1
}

CACHE_KEY="$(source_hash)"
S3_PATH="ci-artifacts/opake/wasm/${CACHE_KEY}"

rclone_s3() {
  rclone --s3-provider Other \
    --s3-endpoint "${S3_ENDPOINT}" \
    --s3-access-key-id "${S3_ACCESS_KEY}" \
    --s3-secret-access-key "${S3_SECRET_KEY}" \
    --s3-no-check-bucket \
    "$@"
}

try_download_artifact() {
  echo "Cache key: ${CACHE_KEY}"
  echo "Checking for cached wasm artifact..."
  mkdir -p "$WASM_OUT"
  if rclone_s3 copy ":s3:${S3_BUCKET}/${S3_PATH}/" "$WASM_OUT/" 2>/dev/null; then
    if [ -f "${WASM_OUT}/opake_bg.wasm" ]; then
      echo "Cache hit — using cached wasm artifact"
      return 0
    fi
  fi
  echo "Cache miss"
  return 1
}

upload_artifact() {
  echo "Uploading wasm artifact (key: ${CACHE_KEY})..."
  rclone_s3 copy "$WASM_OUT/" ":s3:${S3_BUCKET}/${S3_PATH}/"
  echo "Uploaded"
}

build_wasm() {
  echo "Building wasm..."
  wasm-pack build "$WASM_CRATE" --target web --out-dir "../../${WASM_OUT}" --out-name opake
}

if try_download_artifact; then
  exit 0
fi

build_wasm
upload_artifact
