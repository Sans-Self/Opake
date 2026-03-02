# AppView: API & Deployment

The AppView indexes `app.opake.cloud.grant` and `app.opake.cloud.keyring` records from the AT Protocol firehose and serves them via a REST API. It enables the `inbox` command — "what's been shared with me?" — without scanning every PDS in the network.

## Running Modes

```bash
opake-appview run      # indexer + API (default)
opake-appview index    # indexer only (write-only, no HTTP)
opake-appview serve    # API only (read-only, no Jetstream)
opake-appview status   # print cursor + stats, exit
```

Running with no subcommand is equivalent to `run`.

### Flags

| Flag | Effect |
|------|--------|
| `-v` / `-vv` / `-vvv` | Logging: info / debug / trace |
| `--config-dir <path>` | Override data directory containing `appview.toml` |

## Configuration

TOML file at `~/.config/opake/appview.toml` (or `$XDG_CONFIG_HOME/opake/appview.toml`).

Override: set `OPAKE_DATA_DIR` to the directory containing `appview.toml`, or use `--config-dir`.

```toml
jetstream_url = "wss://jetstream2.us-east.bsky.network/subscribe"
listen = "127.0.0.1:6100"
db_path = "~/.config/opake/appview.db"
```

| Field | Required | Notes |
|-------|----------|-------|
| `jetstream_url` | yes | Must start with `ws://` or `wss://` |
| `listen` | yes | `host:port` for the HTTP server |
| `db_path` | yes | SQLite path. `~` is expanded. |

## Authentication

All API endpoints except `/api/health` require authentication via DID-scoped Ed25519 signatures.

Users prove they control a DID by signing with their opake Ed25519 signing key.

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
1. Parse header — extract DID, timestamp, signature
2. Reject if timestamp is >60 seconds from now (replay protection)
3. Reject if `?did=` parameter doesn't match authenticated DID (scope enforcement)
4. Fetch `app.opake.cloud.publicKey/self` from the user's PDS
5. Extract `signingKey` (Ed25519) from the record
6. Verify signature with `ed25519-dalek`
7. Cache verified key for 5 minutes

**CLI side:**
```rust
let timestamp = Utc::now().timestamp();
let message = format!("GET:/api/inbox:{timestamp}:{did}");
let signature = signing_key.sign(message.as_bytes());
let header = format!("Opake-Ed25519 {did}:{timestamp}:{}", BASE64.encode(signature.to_bytes()));
```

## API Endpoints

### `GET /api/health`

Always unauthenticated. Returns indexer status only — no aggregate data.

```json
{
  "indexerConnected": true,
  "cursorTime": "2026-03-02T12:00:00+00:00",
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
      "uri": "at://did:plc:owner/app.opake.cloud.grant/3abc",
      "ownerDid": "did:plc:owner",
      "documentUri": "at://did:plc:owner/app.opake.cloud.document/3xyz",
      "permissions": "read",
      "note": "photos from the trip",
      "createdAt": "2026-03-01T12:00:00Z"
    }
  ],
  "cursor": "2026-03-01T12:00:01Z::at://did:plc:owner/app.opake.cloud.grant/3abc"
}

```

### `GET /api/keyrings?did=<did>&limit=<n>&cursor=<cursor>`

Returns keyrings where `did` is a member.

```json
{
  "keyrings": [
    {
      "uri": "at://did:plc:owner/app.opake.cloud.keyring/3def",
      "ownerDid": "did:plc:owner",
      "name": "family-photos",
      "indexedAt": "2026-03-01T12:00:00Z"
    }
  ],
  "cursor": "..."
}
```

## Rate Limiting

All endpoints are rate-limited per IP via `tower_governor`. Limits: 10 requests/second sustained, 30 burst. Requests beyond the limit receive `429 Too Many Requests`.

IP extraction checks `X-Forwarded-For`, `X-Real-Ip`, and falls back to peer address — works correctly behind Traefik or similar reverse proxies.

## Horizontal Scaling

SQLite WAL mode supports one writer + many readers. For horizontal scaling:

- **One `index` process** — writes to the database
- **N `serve` processes** — read-only, behind a load balancer

All processes point at the same `db_path`. WAL mode handles concurrent reads during writes.

For setups beyond a single machine, migrate to Postgres (future work).

## Status Command

Quick operational check without starting a server:

```
$ opake-appview status
Cursor:   2026-03-02T12:00:00+00:00
Lag:      5s
Grants:   42
Keyrings: 3
```
