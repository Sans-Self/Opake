#!/usr/bin/env bash
# dev-env bootstrap — runs INSIDE the internal (egress-blocked) network via the
# opake CLI container. For each fixture actor: mint invite → create account →
# derive the encryption identity from the FIXED mnemonic and publish
# app.opake.publicKey/self. All over plain http to the internal PDS, so no TLS
# trust is needed and the PDSes stay strictly internal-only.
#
# Why this shape (not `opake account login`): legacy login GENERATES a fresh
# random identity behind an interactive 3-word confirmation. We instead seed a
# legacy session + account config from the createAccount tokens (the CLI reads
# the PDS URL from account config, NOT from DID-doc resolution — so it stays on
# http://pds-x:3000), then `recover` imports the known mnemonic. With no key yet
# published, recover has no mismatch prompt, so the whole flow is non-interactive.
#
# Assumes a clean (post-reset) network. Run: docker compose run --rm bootstrap \
#   /bootstrap/bootstrap.sh   (LIMIT=alice to do just one actor)
set -euo pipefail

FIXTURES="${FIXTURES:-/fixtures/actors.json}"
ADMIN_PW="${PDS_ADMIN_PASSWORD:?set PDS_ADMIN_PASSWORD}"
PASSWORD="${ACTOR_PASSWORD:-opake-devenv-pw}"
LIMIT="${LIMIT:-}"

WORK="$(mktemp -d)"
export OPAKE_DATA_DIR="$WORK"
export OPAKE_PLC_DIRECTORY="${OPAKE_PLC_DIRECTORY:-http://plc:2582}"
CONFIG="$WORK/config.toml"
: > "$CONFIG"

sanitize() { printf '%s' "$1" | tr ':' '_'; }

first_did=""
n=$(jq '.actors | length' "$FIXTURES")
for i in $(seq 0 $((n - 1))); do
  name=$(jq -r ".actors[$i].name" "$FIXTURES")
  [ -n "$LIMIT" ] && [ "$name" != "$LIMIT" ] && continue
  handle=$(jq -r ".actors[$i].handle" "$FIXTURES")
  pds=$(jq -r ".actors[$i].pds" "$FIXTURES")
  mnemonic=$(jq -r ".actors[$i].mnemonic" "$FIXTURES")
  # Per-actor password from fixtures; env ACTOR_PASSWORD is the fallback.
  password=$(jq -r ".actors[$i].password // empty" "$FIXTURES")
  [ -n "$password" ] || password="$PASSWORD"
  # Namespaced actors (provisioned by the e2e harness against a live dev-env)
  # keep the checked-in role names — the cabinet-seeding rules below key on
  # them — but share a PDS with the default population, so they must carry
  # their own email. Absent the field this is the historical derivation.
  email=$(jq -r ".actors[$i].email // empty" "$FIXTURES")
  [ -n "$email" ] || email="${name}@${pds}.test"
  base="http://${pds}:3000"
  echo "== ${name}  (${handle} @ ${pds}) =="

  invite=$(curl -fsS -u "admin:${ADMIN_PW}" -X POST \
    "$base/xrpc/com.atproto.server.createInviteCode" \
    -H 'content-type: application/json' -d '{"useCount":1}' | jq -r .code)
  echo "   invite: $invite"

  resp=$(curl -fsS -X POST "$base/xrpc/com.atproto.server.createAccount" \
    -H 'content-type: application/json' \
    -d "{\"handle\":\"${handle}\",\"email\":\"${email}\",\"password\":\"${password}\",\"inviteCode\":\"${invite}\"}")
  did=$(echo "$resp" | jq -r .did)
  access=$(echo "$resp" | jq -r .accessJwt)
  refresh=$(echo "$resp" | jq -r .refreshJwt)
  if [ "$did" = null ] || [ -z "$did" ]; then
    echo "   createAccount FAILED: $resp"; exit 1
  fi
  echo "   did: $did"

  san=$(sanitize "$did")
  mkdir -p "$WORK/accounts/$san"
  printf '{"type":"legacy","did":"%s","handle":"%s","access_jwt":"%s","refresh_jwt":"%s"}\n' \
    "$did" "$handle" "$access" "$refresh" > "$WORK/accounts/$san/session.json"

  printf '[accounts."%s"]\npds_url = "%s"\nhandle = "%s"\n\n' \
    "$did" "$base" "$handle" >> "$CONFIG"
  [ -z "$first_did" ] && first_did="$did"

  # Import the fixed mnemonic and publish the encryption public key.
  echo "$mnemonic" | opake --as "$did" recover >/dev/null
  echo "   published app.opake.publicKey/self"

  # Genesis-create the cabinet root directory with a first write. A recovered
  # cabinet has no root until something is written; `opake ls`/`mkdir` fail with
  # "no root directory" until then (mkdir creates an orphan child, not the root
  # — only a document upload materializes it). Most actors are seeded so the
  # cabinet specs load against a ready tree.
  #
  # `frank` is left DELIBERATELY UNSEEDED: its cabinet has no root, modelling a
  # recovered web-only user who has never written. The cabinet-fresh-root spec
  # uses frank to prove the web's first write now creates the root on demand
  # (core's ensure_root, no JS-side root construction). Frank is otherwise
  # unused — workers map only to alice/bob/carol/dave (playwright workers: 4).
  if [ "$name" = "frank" ]; then
    echo "   left cabinet UNSEEDED (fresh web-only-user fixture)"
    continue
  fi
  seed="$WORK/.cabinet-init"
  printf 'opake cabinet root seed\n' > "$seed"
  opake --as "$did" upload "$seed" >/dev/null
  echo "   seeded cabinet root"
done

# default_did for convenience (CLI verification uses it); optional for --account.
if [ -n "$first_did" ]; then
  printf 'default_did = "%s"\n\n%s' "$first_did" "$(cat "$CONFIG")" > "$CONFIG"
fi
echo "== bootstrap complete =="
