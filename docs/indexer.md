<!-- 
  NOTE TO EDITORS: 
  Opake uses a dual-documentation system. If you modify the Indexer service 
  details or indexing logic in this file, you MUST also update the 
  corresponding MDX content in `apps/web/src/content/` to prevent documentation drift. 
-->

# Indexer: API & Deployment

The Indexer indexes five `app.opake.*` collections from the AT Protocol firehose — `grant`, `keyring`, `document` (keyring-encrypted only), `documentUpdate`, and `keyringLeave` — and serves them via a REST API. It enables the `inbox` command ("what's been shared with me?") and workspace queries without scanning every PDS in the network.

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
| `keyring_members` | `(keyring_uri, member_did)` | Denormalized keyring membership with role |
| `workspace_documents` | `document_uri` | Documents encrypted under a keyring |
| `document_updates` | `uri` | Pending collaborative edit proposals |

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

Returns pending document updates. If `document` is provided, returns updates for that specific document. If omitted, returns all pending updates targeting documents owned by the authenticated DID (joined on workspace_documents ownership).

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

## Firehose Collections

| Collection | Events | Effect |
|------------|--------|--------|
| `app.opake.grant` | create/update/delete | Index/remove grants in `grants` table |
| `app.opake.keyring` | create/update/delete | Upsert/delete keyring members (with roles) in `keyring_members` |
| `app.opake.document` | create/update/delete | If `keyringEncryption`, index in `workspace_documents`. Direct-encrypted documents are ignored. |
| `app.opake.documentUpdate` | create/update/delete | Index/remove in `document_updates` |
| `app.opake.keyringLeave` | create | Remove the authoring member from `keyring_members` for the referenced keyring |

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
