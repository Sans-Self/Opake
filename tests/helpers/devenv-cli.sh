#!/usr/bin/env bash
# In-container helper for the federation test tier (tests/helpers/devenv.ts).
# Runs inside the dev-env's opake CLI container, where service names resolve.
#
#   login <name>        seed a legacy session + import the fixed mnemonic for a
#                       fixture actor (accounts already exist via bootstrap.sh),
#                       matching bootstrap's non-interactive shape; prints DID
#   delkr <name> <rkey> delete one app.opake.keyring record over XRPC, authored
#                       by the actor's own session — drives genesis / sole-record
#                       deletes without a dedicated CLI verb
#   delpubkey <name>    stash then delete an actor's app.opake.publicKey/self
#                       record — makes a fully-seeded fixture actor "not ready"
#                       so a share to them hits RecipientNotReady (the condition
#                       is otherwise unreachable: bootstrap seeds every actor)
#   putpubkey <name>    restore the publicKey/self record stashed by delpubkey,
#                       returning the actor to "ready" (recipient can be shared to)
#   backdate <name> <collection> <rkey> <iso>
#                       rewrite one record's createdAt in place (getRecord →
#                       jq → putRecord), preserving all other fields including
#                       real encrypted payloads — ages a pending share past its
#                       TTL without a 7-day wait or a hand-built fake record
set -euo pipefail

FIXTURES=/fixtures/actors.json
cmd="$1"; shift

case "$cmd" in
  login)
    name="$1"; dir="/work/$name"; mkdir -p "$dir"
    handle=$(jq -r ".actors[]|select(.name==\"$name\").handle" "$FIXTURES")
    pds=$(jq -r ".actors[]|select(.name==\"$name\").pds" "$FIXTURES")
    mnemonic=$(jq -r ".actors[]|select(.name==\"$name\").mnemonic" "$FIXTURES")
    password=$(jq -r ".actors[]|select(.name==\"$name\").password" "$FIXTURES")
    base="http://${pds}:3000"
    resp=$(curl -fsS -X POST "$base/xrpc/com.atproto.server.createSession" \
      -H 'content-type: application/json' \
      -d "{\"identifier\":\"$handle\",\"password\":\"$password\"}")
    did=$(echo "$resp" | jq -r .did)
    san=${did//:/_}
    mkdir -p "$dir/accounts/$san"
    printf '{"type":"legacy","did":"%s","handle":"%s","access_jwt":"%s","refresh_jwt":"%s"}\n' \
      "$did" "$handle" "$(echo "$resp" | jq -r .accessJwt)" "$(echo "$resp" | jq -r .refreshJwt)" \
      > "$dir/accounts/$san/session.json"
    printf 'default_did = "%s"\n\n[accounts."%s"]\npds_url = "%s"\nhandle = "%s"\n' \
      "$did" "$did" "$base" "$handle" > "$dir/config.toml"
    printf '%s\n' "$base" > "$dir/.pds"
    printf '%s\n' "$did" > "$dir/.did"
    printf '%s\n' "$mnemonic" | OPAKE_DATA_DIR="$dir" opake recover >/dev/null 2>&1
    echo "$did"
    ;;
  delkr)
    name="$1"; rkey="$2"; dir="/work/$name"
    did=$(cat "$dir/.did"); base=$(cat "$dir/.pds"); san=${did//:/_}
    jwt=$(jq -r .access_jwt "$dir/accounts/$san/session.json")
    curl -fsS -X POST "$base/xrpc/com.atproto.repo.deleteRecord" \
      -H "authorization: Bearer $jwt" -H 'content-type: application/json' \
      -d "{\"repo\":\"$did\",\"collection\":\"app.opake.keyring\",\"rkey\":\"$rkey\"}" >/dev/null
    echo ok
    ;;
  delpubkey)
    name="$1"; dir="/work/$name"
    did=$(cat "$dir/.did"); base=$(cat "$dir/.pds"); san=${did//:/_}
    jwt=$(jq -r .access_jwt "$dir/accounts/$san/session.json")
    # Stash the current record value so putpubkey can restore it verbatim.
    curl -fsS "$base/xrpc/com.atproto.repo.getRecord?repo=$did&collection=app.opake.publicKey&rkey=self" \
      | jq -c '.value' > "$dir/.pubkey.json"
    curl -fsS -X POST "$base/xrpc/com.atproto.repo.deleteRecord" \
      -H "authorization: Bearer $jwt" -H 'content-type: application/json' \
      -d "{\"repo\":\"$did\",\"collection\":\"app.opake.publicKey\",\"rkey\":\"self\"}" >/dev/null
    echo ok
    ;;
  putpubkey)
    name="$1"; dir="/work/$name"
    did=$(cat "$dir/.did"); base=$(cat "$dir/.pds"); san=${did//:/_}
    jwt=$(jq -r .access_jwt "$dir/accounts/$san/session.json")
    record=$(cat "$dir/.pubkey.json")
    jq -n --arg repo "$did" --argjson record "$record" \
      '{repo:$repo,collection:"app.opake.publicKey",rkey:"self",record:$record}' \
      | curl -fsS -X POST "$base/xrpc/com.atproto.repo.putRecord" \
        -H "authorization: Bearer $jwt" -H 'content-type: application/json' \
        -d @- >/dev/null
    echo ok
    ;;
  backdate)
    name="$1"; coll="$2"; rkey="$3"; iso="$4"; dir="/work/$name"
    did=$(cat "$dir/.did"); base=$(cat "$dir/.pds"); san=${did//:/_}
    jwt=$(jq -r .access_jwt "$dir/accounts/$san/session.json")
    value=$(curl -fsS "$base/xrpc/com.atproto.repo.getRecord?repo=$did&collection=$coll&rkey=$rkey" \
      | jq -c --arg iso "$iso" '.value | .createdAt = $iso')
    jq -n --arg repo "$did" --arg coll "$coll" --arg rkey "$rkey" --argjson record "$value" \
      '{repo:$repo,collection:$coll,rkey:$rkey,record:$record}' \
      | curl -fsS -X POST "$base/xrpc/com.atproto.repo.putRecord" \
        -H "authorization: Bearer $jwt" -H 'content-type: application/json' \
        -d @- >/dev/null
    echo ok
    ;;
  *)
    echo "unknown helper cmd: $cmd" >&2; exit 2
    ;;
esac
