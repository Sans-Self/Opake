<!-- 
  NOTE TO EDITORS: 
  Opake uses a dual-documentation system. If you modify the Indexer service 
  details or indexing logic in this file, you MUST also update the 
  corresponding MDX content in `apps/web/src/content/` to prevent documentation drift. 
-->

# Indexer: API & Deployment

The Indexer ingests the eight `app.opake.*` collections from the AT Protocol firehose and serves them via a REST API plus a Server-Sent Events stream. It backs the `inbox` query ("what's been shared with me?"), workspace document / directory discovery, and the live update pipeline that keeps web and CLI clients in sync without polling.

Built with Elixir/Phoenix. Source lives in `apps/indexer/`.

## Running Modes

Controlled by environment variables, not subcommands:

| Mode | Config | Effect |
|------|--------|--------|
| `run` (default) | — | Indexer + API |
| `serve` | `INDEXER_ENABLED=false` | API only (no Jetstream) |
| `index` | `PHX_SERVER=false` | Indexer only (no HTTP) |

Status check via release eval:

```bash
bin/opake_indexer eval "OpakeIndexer.Release.status()"
```

## Development

```bash
cd apps/indexer
docker compose up -d          # start postgres
mix setup                     # deps + create DB + migrate
mix phx.server                # dev server on :6100
mix test                      # run tests
```

## Production (Docker)

```bash
cd apps/indexer
docker compose --profile full up --build
```

This starts postgres and the indexer container. The entrypoint auto-creates the database and runs migrations.

## Database Schema

| Table | PK | Purpose |
|-------|-----|---------|
| `cursor` | `id` (singleton) | Jetstream cursor position |
| `grants` | `uri` | Indexed sharing grants |
| `keyrings` | `uri` | Keyring records (owner, metadata) |
| `keyring_members` | `(keyring_uri, member_did)` | Denormalized membership with role |
| `keyring_updates` | `uri` | Membership proposals (add/remove member) |
| `documents` | `uri` | Keyring-encrypted documents (join target for workspace ops) |
| `document_updates` | `uri` | Pending collaborative edit proposals |
| `directories` | `uri` | Workspace directory records |
| `directory_updates` | `uri` | Pending structural proposals (add/move/rename/delete entry) |

## Configuration

### Development

`config/dev.exs` — defaults to local postgres (`postgres:postgres@localhost/opake_indexer_dev`) and the public Jetstream relay.

### Production (environment variables)

| Variable | Required | Default | Notes |
|----------|----------|---------|-------|
| `DATABASE_URL` | yes | — | Ecto URL, e.g. `ecto://user:pass@host/db` |
| `SECRET_KEY_BASE` | yes | — | 64+ char random string |
| `JETSTREAM_URL` | no | dev default | Must start with `ws://` or `wss://` |
| `PORT` | no | `6100` | HTTP listen port |
| `PHX_HOST` | no | `localhost` | Hostname for URL generation |
| `PHX_SERVER` | no | `true` | Set to `false` to disable HTTP |
| `INDEXER_ENABLED` | no | `true` | Set to `false` to disable Jetstream consumer |
| `POOL_SIZE` | no | `10` | Postgres connection pool size |
| `ECTO_IPV6` | no | `false` | Enable IPv6 for Postgres connections |

## Authentication

All API endpoints except `/api/health` require authentication via DID-scoped Ed25519 signatures.

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

### `GET /api/health`

Always unauthenticated. Returns indexer status only — no aggregate data.

```json
{
  "indexerConnected": true,
  "cursorTime": "2026-03-02T12:00:00.000000Z",
  "cursorAgeSecs": 5
}
```

### `GET /api/inbox?did=<did>&limit=<n>&cursor=<cursor>`

Returns grants where `did` is the recipient. Newest first.

| Param | Required | Default | Max |
|-------|----------|---------|-----|
| `did` | yes | — | — |
| `limit` | no | 50 | 100 |
| `cursor` | no | — | — |

```json
{
  "grants": [
    {
      "uri": "at://did:plc:owner/app.opake.grant/3abc",
      "ownerDid": "did:plc:owner",
      "documentUri": "at://did:plc:owner/app.opake.document/3xyz",
      "createdAt": "2026-03-01T12:00:00Z"
    }
  ],
  "cursor": "2026-03-01T12:00:01.000000Z::at://did:plc:owner/app.opake.grant/3abc"
}
```

### `GET /api/keyrings?did=<did>&limit=<n>&cursor=<cursor>`

Returns keyrings where `did` is a member.

```json
{
  "keyrings": [
    {
      "uri": "at://did:plc:owner/app.opake.keyring/3def",
      "ownerDid": "did:plc:owner",
      "indexedAt": "2026-03-01T12:00:00.000000Z"
    }
  ],
  "cursor": "..."
}
```

### `GET /api/workspace?keyring=<uri>&limit=<n>&cursor=<cursor>`

Returns documents encrypted under a keyring. **Requires the authenticated DID to be a member of the keyring** (returns 403 otherwise).

| Param | Required | Default | Max |
|-------|----------|---------|-----|
| `keyring` | yes | — | — |
| `limit` | no | 50 | 100 |
| `cursor` | no | — | — |

```json
{
  "documents": [
    {
      "documentUri": "at://did:plc:alice/app.opake.document/3abc",
      "keyringUri": "at://did:plc:owner/app.opake.keyring/3def",
      "ownerDid": "did:plc:alice",
      "rotation": 0,
      "indexedAt": "2026-03-21T10:00:00.000000Z"
    }
  ],
  "cursor": "..."
}
```

### `GET /api/workspace/updates?document=<uri>&limit=<n>&cursor=<cursor>`

Returns pending document updates. If `document` is provided, returns updates for that specific document. If omitted, returns all pending updates targeting documents owned by the authenticated DID (joined on `documents` ownership).

| Param | Required | Default | Max |
|-------|----------|---------|-----|
| `document` | no | — | — |
| `limit` | no | 50 | 100 |
| `cursor` | no | — | — |

```json
{
  "updates": [
    {
      "uri": "at://did:plc:editor/app.opake.documentUpdate/3abc",
      "documentUri": "at://did:plc:owner/app.opake.document/3xyz",
      "authorDid": "did:plc:editor",
      "supersedesUri": null,
      "indexedAt": "2026-03-21T10:00:00.000000Z"
    }
  ]
}
```

### `GET /api/workspace/directory-updates?keyring=<uri>&limit=<n>&cursor=<cursor>`

Returns pending structural proposals for a workspace (add/move/rename/delete entry). Requires the authenticated DID to be a member of the keyring.

### Tree snapshots and sync

`/api/cabinet/snapshot` and `/api/workspace/snapshot?keyring=<uri>` return the full directory tree + document list for cold starts. `/api/cabinet/sync?since=<iso8601>` and `/api/workspace/sync?keyring=<uri>&since=<iso8601>` return only records whose `indexed_at` is newer than `since`, for incremental catch-up after a reconnect. Both sync endpoints reply with `{ directories, documents, serverTime }`; the caller uses `serverTime` as the next `since` value.

### SSE event streaming

Two endpoints work together to push indexed events to authenticated consumers in real time.

`POST /api/events/token` (Ed25519-authenticated) returns a short-lived single-use token:

```json
{ "token": "<opaque>", "ttl": 60 }
```

`GET /api/events?token=<opaque>` upgrades to a chunked text/event-stream response. The consumer subscribes to:
- their personal topic (keyring memberships affecting them, grants addressed to them, their own proposals)
- every workspace keyring they are currently a member of (re-computed dynamically as `keyring:upsert` events flow through)

Events are formatted as:

```
event: <type>
data: <json payload>

```

Types include `keyring:upsert` / `keyring:delete`, `grant:upsert` / `grant:delete`, `directory:upsert` / `directory:delete`, `document:upsert` / `document:delete`, and the matching `*:proposal` events for the update collections. A keepalive comment is emitted every 15 seconds to survive proxy idle timeouts.

Each DID is capped at a small number of concurrent SSE connections (tracked in ETS); additional connections return `429`. Token exchange is one-shot — the consumer must POST again after losing the connection.

## Firehose Collections

| Collection | Events | Effect |
|------------|--------|--------|
| `app.opake.grant` | create/update/delete | Index/remove in `grants` |
| `app.opake.keyring` | create/update/delete | Upsert `keyrings` row; replace `keyring_members` (with roles) |
| `app.opake.keyringUpdate` | create/update/delete | Index/remove in `keyring_updates` (add/remove-member proposals) |
| `app.opake.document` | create/update/delete | Keyring-encrypted documents → `documents`. Direct-encrypted documents are ignored by the indexer. |
| `app.opake.documentUpdate` | create/update/delete | Index/remove in `document_updates` |
| `app.opake.directory` | create/update/delete | Upsert/delete in `directories` |
| `app.opake.directoryUpdate` | create/update/delete | Index/remove in `directory_updates` (add/move/rename/delete entry proposals) |
| `app.opake.accountConfig` | create/update/delete | Parsed as a proof-of-life heartbeat; not persisted |

## Rate Limiting

All endpoints are rate-limited per IP via Hammer (ETS backend). Limits: 30 requests/second burst. Requests beyond the limit receive `429 Too Many Requests`.

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
