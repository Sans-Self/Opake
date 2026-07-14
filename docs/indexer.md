<!-- 
  NOTE TO EDITORS: 
  Opake uses a dual-documentation system. If you modify the Indexer service 
  details or indexing logic in this file, you MUST also update the 
  corresponding MDX content in `apps/web/src/content/` to prevent documentation drift. 
-->

# Indexer: API & Deployment

The Indexer ingests `app.opake.*` records from the AT Protocol firehose and serves them via a REST API plus a Server-Sent Events stream. It backs the `inbox` query ("what's been shared with me?"), workspace document / directory discovery, the chain-head lookup a client needs before it can write a supersede, and the live update pipeline that keeps web and CLI clients in sync without polling.

It also enforces write authority. Members write curatorial supersedes directly to their own PDS; the indexer decides whether a supersede is allowed to advance a chain (`Authority`), and clients mirror the same rules as defense in depth. There is no proposal system.

Built with Elixir/Phoenix. Source lives in `apps/indexer/`.

## Running Modes

Controlled by environment variables, not subcommands:

| Mode | Config | Effect |
|------|--------|--------|
| `run` | `PHX_SERVER=true` | Indexer + API. `mix phx.server` and the compose file both set this |
| `serve` | `PHX_SERVER=true`, `INDEXER_ENABLED=false` | API only (no Jetstream) |
| `index` | `PHX_SERVER` unset or `false` | Indexer only (no HTTP) |

Status check via release eval:

```bash
bin/opake_indexer eval "OpakeIndexer.Release.status()"
```

## Development

The compose file lives at the repo root, not under `apps/indexer`:

```bash
docker compose up -d          # start postgres
cd apps/indexer
mix setup                     # deps + create DB + migrate
mix phx.server                # dev server on :6100
mix test                      # run tests
```

## Production (Docker)

```bash
docker compose --profile full up --build
```

This starts postgres and the indexer container. The entrypoint (`apps/indexer/rel/entrypoint.sh`) creates the database if needed and runs migrations before booting the release.

## Database Schema

Three tables. Records are stored as one row per AT-URI regardless of collection, with the verbatim PDS record JSON in `record_jsonb` — the structural columns are projections of that JSON, maintained by the firehose dispatch so lookups can be index-driven.

| Table | PK | Purpose |
|-------|-----|---------|
| `cursor` | `id` (singleton) | Jetstream cursor position |
| `records` | `uri` | Every indexed `app.opake.*` record. Columns: `collection`, `author_did`, `workspace_id`, `supersedes_uri`, `is_workspace_root`, `cid`, `indexed_at`, `updated_at`, `deleted_at`, `record_jsonb` |
| `chain_heads` | `(workspace_id, kind)` | Current head of a tracked chain. `kind` is `keyring` or `workspace_root`; carries `head_uri` + `head_cid` |

Deletes are soft: a tombstone sets `deleted_at` and the row stays, so a client syncing from a cursor still sees that the record went away. `TombstoneCleanup` purges tombstones older than 7 days, hourly.

`indexed_at` is first-seen and immutable — it orders pagination and nothing else. `updated_at` is the last-write watermark and is what the sync endpoints compare against.

## Configuration

### Development

`config/dev.exs` — defaults to local postgres (`postgres:postgres@localhost/opake_indexer_dev`) and the public Jetstream relay, with `firehose_mode: :opake_only` so a dev machine only receives the collections it can act on.

### Production (environment variables)

| Variable | Required | Default | Notes |
|----------|----------|---------|-------|
| `DATABASE_URL` | yes | — | Ecto URL, e.g. `ecto://user:pass@host/db` |
| `SECRET_KEY_BASE` | yes | — | 64+ char random string |
| `JETSTREAM_URL` | no | dev default | Must start with `ws://` or `wss://` |
| `PLC_DIRECTORY_URL` | no | `https://plc.directory` | DID resolution for key fetches |
| `CORS_ORIGIN` | no | — | Allowed browser origin; dev uses `*` |
| `PORT` | no | `6100` | HTTP listen port |
| `PHX_HOST` | no | `localhost` | Hostname for URL generation |
| `PHX_SERVER` | no | unset | A release starts HTTP only when this is set to something other than `false`/`0`. `mix phx.server` sets it for you; the compose file sets it explicitly |
| `INDEXER_ENABLED` | no | `true` | Set to `false` to disable the Jetstream consumer |
| `POOL_SIZE` | no | `10` | Postgres connection pool size |
| `ECTO_IPV6` | no | `false` | Enable IPv6 for Postgres connections |

## Authentication

Every API endpoint except `/api/health` and `/api/events` requires authentication via DID-scoped Ed25519 signatures. `/api/events` is the SSE stream: it sits outside the authenticated pipeline and takes a single-use token instead, because `EventSource` cannot send custom headers.

**Header format:**
```
Authorization: Opake-Ed25519 <did>:<unix-timestamp>:<base64(signature)>
```

**Signature covers:**
```
<METHOD>:<path>:<timestamp>:<did>
```

Example:
```
GET:/api/inbox:1709330400:did:plc:abc123
```

**Verification flow:**
1. Parse header — extract DID, timestamp, signature (split from right, DIDs contain colons)
2. Reject if timestamp is >60 seconds from now (replay protection)
3. Reject if `?did=` parameter doesn't match authenticated DID (scope enforcement)
4. Fetch `app.opake.publicKey/self` from the user's PDS
5. Extract `signingKey` (Ed25519) from the record
6. Verify signature with Erlang `:crypto` (Ed25519)
7. Cache verified key in ETS for 5 minutes

## API Endpoints

### The record envelope

Every endpoint that returns records returns them in one envelope shape. The `record` field is byte-identical to what the PDS holds — no field flattening, no per-collection projections — with indexer metadata as siblings:

```json
{
  "uri": "at://did:plc:owner/app.opake.document/3abc",
  "record": { "…": "verbatim PDS record JSON" },
  "indexedAt": "2026-03-21T10:00:00.000000Z",
  "deletedAt": "2026-03-22T09:00:00.000000Z"
}
```

`deletedAt` is present only on tombstones, which the sync endpoints deliver so a reconnecting client learns about deletions it slept through. Snapshots never carry tombstones.

### `GET /api/health`

Always unauthenticated. Indexer state only — enough for an operator to tell "connected and flowing" from "connected but idle" from "dead". Row counts are deliberately absent; those are internal metrics.

```json
{
  "indexer_connected": true,
  "cursor_time": "2026-04-06T12:34:56.789Z",
  "cursor_age_secs": 2,
  "events": {
    "total": 12470,
    "indexed": 14,
    "ignored": 12456,
    "last_event_age_ms": 320
  },
  "per_collection": {
    "app.bsky.feed.post": 11200,
    "app.opake.document": 4
  }
}
```

### `GET /api/inbox?limit=<n>&cursor=<cursor>`

Grants naming the authenticated DID as recipient, newest first. The recipient is the caller — there is no `did` parameter to pass. (The auth plug does check a `?did=` query param against the authenticated DID if one is present, but no endpoint requires it.)

| Param | Required | Default | Max |
|-------|----------|---------|-----|
| `limit` | no | 50 | 100 |
| `cursor` | no | — | — |

The cursor is opaque: `"{indexed_at}::{uri}"` of the last item on the page. It is absent from the response on the last page.

```json
{
  "grants": [
    {
      "uri": "at://did:plc:owner/app.opake.grant/3abc",
      "record": { "…": "verbatim grant record" },
      "indexedAt": "2026-03-01T12:00:00.000000Z"
    }
  ],
  "cursor": "2026-03-01T12:00:00.000000Z::at://did:plc:owner/app.opake.grant/3abc"
}
```

### `GET /api/keyrings`

The current keyring head for every workspace the authenticated DID is a member of. Takes no parameters; membership comes from the authenticated DID, matched against the head keyring's `members` array via JSONB containment. Not paginated — a member's workspace count is bounded by atproto practicalities.

Note the response key is `workspaces`, not `keyrings`: the keyring is the wire format, the workspace is the domain concept.

```json
{
  "workspaces": [
    {
      "uri": "at://did:plc:owner/app.opake.keyring/3def",
      "record": { "…": "verbatim keyring record, members array intact" },
      "indexedAt": "2026-03-01T12:00:00.000000Z"
    }
  ]
}
```

### Tree snapshots and sync

`GET /api/cabinet/snapshot` returns the authenticated DID's personal (non-workspace) directories and documents. `GET /api/workspace/snapshot?workspace_id=<genesis-keyring-uri>` returns a workspace's tree. Both are full cold-start reads and exclude tombstones.

`GET /api/cabinet/sync?since=<iso8601>` and `GET /api/workspace/sync?workspace_id=<uri>&since=<iso8601>` return the delta for a reconnecting client. A record is in the delta when its `updated_at` or its `deleted_at` is newer than `since` — the last-write watermark, not first-seen. `indexed_at` never moves once assigned, so an in-place update to an old record is delivered without disturbing its pagination order. `since` is required; a missing or unparseable value is a `400`.

Both shapes are the same, with `workspace_id` echoed back on the workspace variant:

```json
{
  "directories": [{ "uri": "…", "record": {}, "indexedAt": "…" }],
  "documents": [{ "uri": "…", "record": {}, "indexedAt": "…" }],
  "server_time": "2026-03-21T10:00:00.000000Z",
  "workspace_id": "at://did:plc:owner/app.opake.keyring/3def"
}
```

The caller passes `server_time` back as the next `since`.

`workspace_id` is the genesis keyring URI — the workspace's identity, which stays fixed as the keyring chain advances. Both spellings are accepted, `workspace_id` and `workspaceId`.

### `GET /api/workspace/chain-head?workspace_id=<uri>`

The workspace's current chain heads. A client reads this to learn what URI to point `supersedes` at before writing the next mutation; getting it wrong is what produces a fork.

Two chains are tracked. Either pointer is `null` when that chain has no head — a workspace with no root directory written yet reports `root_directory: null`.

```json
{
  "workspace_id": "at://did:plc:owner/app.opake.keyring/3def",
  "keyring": {
    "head_uri": "at://did:plc:owner/app.opake.keyring/3ghi",
    "head_cid": "bafy…"
  },
  "root_directory": {
    "head_uri": "at://did:plc:alice/app.opake.directory/3jkl",
    "head_cid": "bafy…"
  }
}
```

### Workspace-scoped response contract

Every workspace-scoped endpoint (`/api/workspace/*`) resolves the caller against the workspace's keyring chain head and answers one of three ways:

| Response | Meaning |
|----------|---------|
| `404` + `{"error": "workspace_not_indexed"}` | No keyring chain head is indexed for the workspace. Covers both a genesis not yet consumed from the firehose and a torn-down chain (no live keyring record) — the indexer cannot tell these apart and does not pretend to. Clients treat this as retryable within a bounded window, branching on the body's error code, never the bare status. |
| `403` | An indexed chain head was consulted and the caller's DID is not in its member list. Definitive authorization denial — never a lag artifact, never retried. |
| `200` | Member; request served. |

### SSE event streaming

Two endpoints work together to push indexed events to authenticated consumers in real time.

`POST /api/events/token` (Ed25519-authenticated) returns a short-lived single-use token:

```json
{ "token": "<opaque>", "ttl": 60 }
```

`GET /api/events?token=<opaque>` upgrades to a chunked text/event-stream response. The consumer subscribes to:
- their personal topic — keyring upserts naming them as a member, grants on either side (sharer and recipient), and their own cabinet records
- every workspace they are currently a member of, re-computed live: an incoming `app.opake.keyring:upsert` that adds them subscribes the open stream to that workspace's topic, and one that drops them unsubscribes it

Events are formatted as:

```
event: <type>
data: <json payload>

```

The event type is the fully-qualified collection name plus an operation suffix: `app.opake.keyring:upsert` / `:delete`, and the same pair for `app.opake.grant`, `app.opake.directory`, and `app.opake.document`. Upsert payloads are the record envelope (`{uri, record, indexedAt}`); delete payloads are `{uri}`. One event type is not a record: `chain:forked`, described below. A keepalive comment is emitted every 15 seconds to survive proxy idle timeouts.

`app.opake.keyring:delete` is the exception to the flat delete payload. It carries `{uri, workspace_id, outcome}` with the resolved chain outcome: `unchanged` (deleted record was not the head), `rolled_back` (head deleted; the chain rolled back to the newest live record, which is re-broadcast as an `app.opake.keyring:upsert` on the same topics), or `torn_down` (no live record remains; the workspace's tracked chains were removed). Clients dispatch on the outcome instead of matching the deleted URI against tracked state — see [flows/keyrings.md](flows/keyrings.md#keyring-record-deletion).

`chain:forked` fires on the workspace topic when a supersede points at a URI that is no longer the head — two members wrote against the same head and one lost. The payload is flat, not a record envelope: `{workspace_id, scope, path, your_uri, fork_point_uri, winner_uri, winner_cid}`, where `scope` is `keyring` or `directory`. The loser's record is persisted but the chain does not advance; the client refetches the winner, replays its intent on top, and retries.

Each DID is capped at five concurrent SSE connections (tracked in ETS); further connections get `429`. Token exchange is one-shot with a 60-second TTL — the consumer must POST again after losing the connection.

## Firehose Collections

Five collections are subscribed; four are indexed. `firehose_mode: :full` (the default) takes every commit Jetstream emits and drops non-`app.opake.*` records at the parser, which keeps proof-of-life logs flowing on a quiet dev machine. `:opake_only` filters server-side to the five below, for low-bandwidth deployments and fast cold-start catch-up.

Every indexed record follows the same path: authority check (for chain-bearing supersedes), upsert into `records`, chain-head update in the same transaction if it participates in a chain, then an SSE broadcast. Deletes soft-delete the row and broadcast a tombstone.

| Collection | Effect |
|------------|--------|
| `app.opake.keyring` | Upsert into `records`; drives the `keyring` chain. Supersedes are authority-checked (manager, or a non-manager writing a pure self-removal) before anything is persisted. Deletes resolve a chain outcome — see the SSE section above |
| `app.opake.directory` | Upsert into `records`. A record flagged `isWorkspaceRoot` drives the `workspace_root` chain; supersedes are authority-checked (manager, or an editor whose supersede is additive). Directories with no `workspaceId` are cabinet records: no chain, no authority check |
| `app.opake.document` | Upsert into `records`; broadcast. No chain |
| `app.opake.grant` | Upsert into `records`; broadcast to both the sharer's and the recipient's personal topic. No chain |
| `app.opake.accountConfig` | Proof-of-life heartbeat; logged, counted, never persisted |

A supersede whose predecessor has not been indexed yet is an orphan: the record is persisted, no chain moves, and the chain heals when the predecessor arrives. A supersede rejected by the authority layer is not persisted at all.

## Rate Limiting

Endpoints are rate-limited per IP via Hammer (ETS backend). Limits: 30 requests/second burst. Requests beyond the limit receive `429 Too Many Requests`. The SSE stream (`/api/events`) is exempt — it is a long-lived connection, not a burst endpoint, and is capped per DID by the connection tracker instead.

IP extraction checks `X-Forwarded-For`, `X-Real-Ip`, and falls back to peer address — works correctly behind reverse proxies.

## Horizontal Scaling

PostgreSQL supports concurrent reads and writes natively. For horizontal scaling:

- **One process with `INDEXER_ENABLED=true`** — consumes the firehose
- **N processes with `INDEXER_ENABLED=false`** — read-only API servers behind a load balancer

All processes share the same `DATABASE_URL`.

## Release Tasks

Available via `bin/opake_indexer eval`:

```bash
# Create the database
bin/opake_indexer eval "OpakeIndexer.Release.create_db()"

# Run pending migrations
bin/opake_indexer eval "OpakeIndexer.Release.migrate()"

# Print cursor position, lag, and indexed record counts
bin/opake_indexer eval "OpakeIndexer.Release.status()"

# Rollback to a specific migration version
bin/opake_indexer eval "OpakeIndexer.Release.rollback(OpakeIndexer.Repo, 20260310000001)"
```
