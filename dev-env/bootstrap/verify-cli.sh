#!/usr/bin/env bash
# CLI verification against the dev-env (opake-dev-env task 3.4): login,
# upload, download as a fixture actor over the local PLC + PDS. Assumes a
# bootstrapped stack. Run like bootstrap.sh:
#   docker compose run --rm bootstrap /bootstrap/verify-cli.sh
set -euo pipefail

FIXTURES="${FIXTURES:-/fixtures/actors.json}"
PASSWORD="${ACTOR_PASSWORD:-opake-devenv-pw}"
ACTOR="${ACTOR:-alice}"

WORK="$(mktemp -d)"
export OPAKE_DATA_DIR="$WORK"
export OPAKE_PLC_DIRECTORY="${OPAKE_PLC_DIRECTORY:-http://plc:2582}"
export OPAKE_INDEXER_URL="${OPAKE_INDEXER_URL:-http://indexer:6100}"

handle=$(jq -r ".actors[] | select(.name == \"$ACTOR\") | .handle" "$FIXTURES")
pds=$(jq -r ".actors[] | select(.name == \"$ACTOR\") | .pds" "$FIXTURES")
mnemonic=$(jq -r ".actors[] | select(.name == \"$ACTOR\") | .mnemonic" "$FIXTURES")
base="http://${pds}:3000"

echo "== login (createSession) as ${handle} =="
resp=$(curl -fsS -X POST "$base/xrpc/com.atproto.server.createSession" \
  -H 'content-type: application/json' \
  -d "{\"identifier\":\"${handle}\",\"password\":\"${PASSWORD}\"}")
did=$(echo "$resp" | jq -r .did)
san=$(printf '%s' "$did" | tr ':' '_')
mkdir -p "$WORK/accounts/$san"
printf '{"type":"legacy","did":"%s","handle":"%s","access_jwt":"%s","refresh_jwt":"%s"}\n' \
  "$did" "$handle" "$(echo "$resp" | jq -r .accessJwt)" "$(echo "$resp" | jq -r .refreshJwt)" \
  > "$WORK/accounts/$san/session.json"
printf 'default_did = "%s"\n\n[accounts."%s"]\npds_url = "%s"\nhandle = "%s"\n' \
  "$did" "$did" "$base" "$handle" > "$WORK/config.toml"

echo "== recover identity from fixture mnemonic =="
echo "$mnemonic" | opake recover >/dev/null

echo "== upload =="
plain="$WORK/verify-$(date +%s).txt"
echo "dev-env cli verification payload" > "$plain"
opake upload "$plain"

echo "== download (cat) and compare =="
# Name resolution walks the directory tree from the indexer, so this leg
# deliberately waits out the pds → relay → jetstream → indexer pipeline.
for attempt in $(seq 1 20); do
  if out="$(opake cat "$(basename "$plain")" 2>/dev/null)"; then break; fi
  [ "$attempt" = 20 ] && { echo "document never became resolvable via indexer"; exit 1; }
  sleep 2
done
[ "$out" = "dev-env cli verification payload" ] || { echo "MISMATCH: $out"; exit 1; }
echo "roundtrip OK (indexed after ~$((attempt * 2))s)"

echo "== cross-PDS resolve (by DID) =="
# Handle→DID for a foreign actor is the known dev-env gap (no DNS for actor
# subdomains inside the blockade; see the design's handle-resolution open
# question). DID-based resolution is fully local: DID doc via the local PLC,
# then the publicKey record from carol's own PDS.
carol_did=$(curl -fsS "http://pds-b:3000/xrpc/com.atproto.identity.resolveHandle?handle=carol.pds-b.test" | jq -r .did)
opake resolve "$carol_did"

echo "== verify-cli PASS =="
