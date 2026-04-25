# Opake — Authentication & Identity

## Authentication

Opake supports two authentication modes against the PDS. OAuth is the default; legacy password auth is a fallback for PDS instances that don't advertise OAuth discovery.

### OAuth Flow

All token handling happens in WASM (opake-core). JS never parses token responses, constructs session objects, or holds DPoP private keys. The WASM exports `startOAuthLogin`, `completeOAuthLogin`, and `loginWithAppPasswordWasm` compose core primitives into complete login flows.

1. Resolve handle to PDS URL (`resolve_pds_for_login` — .well-known, public API, DID document)
2. Discover OAuth Authorization Server via `/.well-known/oauth-protected-resource` → `/.well-known/oauth-authorization-server`; PAR endpoint resolved via `AuthorizationServerMetadata::par_endpoint()`
3. Generate DPoP keypair (P-256/ES256), PKCE S256 challenge, CSRF state
4. Push Authorization Request (PAR) with DPoP proof, PKCE challenge, and `login_hint` (pre-fills the AS consent page)
5. Open browser to authorization URL; CLI starts a loopback HTTP server on `127.0.0.1`
6. User authorizes in the browser; PDS redirects with `code` and `state`
7. CSRF validation, authorization code exchange with DPoP proof + PKCE verifier — all in WASM
8. WASM builds `OAuthSession` and saves to storage. Tokens never enter JS memory.
9. Publish `app.opake.publicKey/self` via idempotent `putRecord`

The web frontend uses a two-step flow: `Opake.startLogin()` returns the auth URL + serializable `PendingLogin` state. The consumer saves this via `Opake.savePendingLogin()` (sessionStorage with 10-minute TTL), redirects, then calls `Opake.completeLogin()` on the callback page. `Opake.loadPendingLogin()` auto-clears the DPoP key material from sessionStorage on read.

### OAuth Scopes

Opake requests granular per-collection scopes instead of the catch-all `transition:generic`:

```
atproto repo:app.opake.accountConfig repo:app.opake.directory ... repo:app.opake.publicKey blob:*/*
```

The scope string is built from `crate::scope::OPAKE_COLLECTIONS` (single source of truth). The same scope is embedded in the loopback client ID via `build_client_id(redirect_uri, scope)` and passed to the PAR body — they must match.

A permission set lexicon (`app.opake.authFullAccess`) bundles all collections for when `include:` scopes are supported by PDSes.

### Legacy Flow

Password-based via `com.atproto.server.createSession`. Used when `--legacy` is passed or OAuth discovery fails. The WASM export `loginWithAppPasswordWasm` handles resolution, authentication, and session storage. Tokens are saved as a `LegacySession`.

### Token Refresh

Proactive. The SDK's `@withTokenGuard` decorator calls `tokenExpiresAt()` (WASM export — returns only the timestamp, no token exposure) before every authenticated operation. If the token expires within 30 seconds, `proactiveRefresh()` (WASM export — calls `refresh_token` directly) is triggered. Concurrent callers share a single-flight promise.

Reactive refresh is the fallback: the XRPC client detects `ExpiredToken` / `AuthenticationFailed` errors and refreshes automatically. DPoP nonces are captured from every response and replayed on subsequent requests.

See [flows/authentication.md](flows/authentication.md) for full sequence diagrams.

## Multi-Account Support

The `Config` type tracks all logged-in accounts with a `default_did` pointer. Each account gets isolated storage:

```
~/.config/opake/
  config.toml               # { default_did, accounts: { did → { handle, pds_url } } }
  accounts/
    did:plc:alice/
      identity.json          # X25519 + Ed25519 keypairs (0600)
      session.json           # OAuth or legacy tokens (0600)
      keyrings/              # Cached group keys per keyring
    did:plc:bob/
      identity.json
      session.json
      keyrings/
```

On the web, `IndexedDbStorage` uses the same logical layout over IndexedDB tables, keyed by DID.

CLI commands that need an account resolve it via: explicit `--as <did>` flag, then `default_did` from config, then error. `opake account list` lists all accounts (`*` marks the default); `opake account set-default <did>` switches.

## Device Pairing

Transfers an encryption identity from an existing device to a new one, using the PDS as a relay. Both devices must be authenticated to the same DID.

```mermaid
sequenceDiagram
    participant New as New Device
    participant PDS
    participant Old as Existing Device

    New->>New: Generate ephemeral X25519 keypair
    New->>Storage: save_pair_state(did, rkey, privkey)
    New->>PDS: createRecord(pairRequest) { ephemeralKey }
    New->>New: Display fingerprint (first 8 bytes, hex)

    Old->>PDS: listRecords(pairRequest)
    Old->>Old: User selects request by fingerprint
    Old->>Old: Encrypt identity with AES-256-GCM + wrap key to ephemeral pubkey
    Old->>PDS: createRecord(pairResponse) { wrappedKey, ciphertext, nonce }

    New->>PDS: Poll for matching pairResponse
    New->>Storage: load_pair_state(did, rkey)
    New->>New: Unwrap key, decrypt identity
    New->>PDS: getRecord(publicKey/self)
    New->>New: Verify derived pubkey == published pubkey
    New->>Storage: save_identity + delete_pair_state
    New->>PDS: Delete pairRequest + pairResponse
```

The ephemeral private key is persisted to the new device's `Storage` (0600 file on CLI, dedicated IndexedDB table on web) between `create_pair_request` and `try_complete_pair` — it needs to survive page reloads or CLI restarts while the user walks to the other device, so in-memory only isn't sufficient. It never crosses the WASM/JS boundary: the SDK exposes `Opake.createPairRequest(storage, did)` → `{uri, rkey, ephemeralPublicKey}` and `Opake.awaitPairCompletion(storage, did, rkey)` → `void`. JS sees the public key (for fingerprint display) and nothing else. The identity payload uses the same AES-256-GCM + x25519-hkdf-a256kw primitives as file encryption. Visual fingerprint comparison is the current SAS mechanism; programmatic verification is a follow-up.

Login on a new device detects an existing `publicKey/self` record and prompts for pairing instead of generating a new keypair (which would orphan encryption on the existing device).

See [flows/pairing.md](flows/pairing.md) for detailed sequence diagrams.
