# AppView: API & Deployment

The AppView indexes `app.opake.grant` and `app.opake.keyring` records from the AT Protocol firehose and serves them via a REST API. It enables the `inbox` command — "what's been shared with me?" — without scanning every PDS in the network.

Built with Elixir/Phoenix. Source lives in `appview/`.

## Running Modes

Controlled by environment variables, not subcommands:

| Mode | Config | Effect |
|------|--------|--------|
| `run` (default) | — | Indexer + API |
| `serve` | `INDEXER_ENABLED=false` | API only (no Jetstream) |
| `index` | `PHX_SERVER=false` | Indexer only (no HTTP) |

Status check via release eval:

```bash
bin/opake_appview eval "OpakeAppview.Release.status()"
```

## Development

```bash
cd appview
docker compose up -d          # start postgres
mix setup                     # deps + create DB + migrate
mix phx.server                # dev server on :6100
mix test                      # run tests
```

## Production (Docker)

```bash
cd appview
docker compose --profile full up --build
```

This starts postgres and the appview container. The entrypoint auto-creates the database and runs migrations.

## Configuration

### Development

`config/dev.exs` — defaults to local postgres (`postgres:postgres@localhost/opake_appview_dev`) and the public Jetstream relay.

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

## Rate Limiting

All endpoints are rate-limited per IP via Hammer (ETS backend). Limits: 30 requests/second burst. Requests beyond the limit receive `429 Too Many Requests`.

IP extraction checks `X-Forwarded-For`, `X-Real-Ip`, and falls back to peer address — works correctly behind reverse proxies.

## Horizontal Scaling

PostgreSQL supports concurrent reads and writes natively. For horizontal scaling:

- **One process with `INDEXER_ENABLED=true`** — consumes the firehose
- **N processes with `INDEXER_ENABLED=false`** — read-only API servers behind a load balancer

All processes share the same `DATABASE_URL`.

## Release Tasks

Available via `bin/opake_appview eval`:

```bash
# Create the database
bin/opake_appview eval "OpakeAppview.Release.create_db()"

# Run pending migrations
bin/opake_appview eval "OpakeAppview.Release.migrate()"

# Print cursor position, lag, and indexed record counts
bin/opake_appview eval "OpakeAppview.Release.status()"

# Rollback to a specific migration version
bin/opake_appview eval "OpakeAppview.Release.rollback(OpakeAppview.Repo, 20260310000001)"
```
