# Authentication

## Login (OAuth)

Default login flow. Uses AT Protocol OAuth 2.0 with DPoP (Demonstrating Proof-of-Possession) and a loopback redirect server for the browser-based authorization.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Browser
    participant PDS/AS

    User->>CLI: opake login <handle>
    CLI->>CLI: Resolve PDS from handle (public API → DID doc)

    CLI->>PDS/AS: GET /.well-known/oauth-protected-resource
    PDS/AS-->>CLI: { authorization_servers: [<as_url>] }
    CLI->>PDS/AS: GET /.well-known/oauth-authorization-server
    PDS/AS-->>CLI: AS metadata (endpoints, PAR, DPoP algs)

    CLI->>CLI: Generate DPoP keypair (P-256), PKCE S256, state

    CLI->>PDS/AS: POST /oauth/par (DPoP proof, PKCE challenge, redirect_uri)
    PDS/AS-->>CLI: { request_uri, expires_in }

    CLI->>CLI: Start loopback server on 127.0.0.1
    CLI->>Browser: Open authorization URL
    Browser->>PDS/AS: User authorizes
    PDS/AS->>Browser: Redirect to 127.0.0.1/callback?code=...&state=...
    Browser->>CLI: GET /callback?code=...&state=...

    CLI->>CLI: Verify state (CSRF check)
    CLI->>PDS/AS: POST /oauth/token (code, PKCE verifier, DPoP proof)
    PDS/AS-->>CLI: { access_token, refresh_token, sub }

    CLI->>CLI: Save OAuthSession (tokens + DPoP key)
    CLI->>CLI: Load or generate X25519 keypair

    CLI->>PDS/AS: com.atproto.repo.putRecord (publicKey/self, DPoP auth)
    PDS/AS-->>CLI: { uri, cid }

    CLI->>User: Logged in as <handle> (OAuth)
```

The loopback server times out after `expires_in` seconds (from the PAR response). If OAuth discovery fails, the CLI falls back to legacy password authentication with a warning.

## Login (Legacy)

Password-based authentication via `createSession`. Used when `--legacy` is passed or when the PDS doesn't support OAuth discovery.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS

    User->>CLI: opake login <handle> --legacy
    CLI->>CLI: Resolve PDS from handle
    CLI->>User: Password prompt (or OPAKE_PASSWORD env)
    User-->>CLI: password

    CLI->>PDS: com.atproto.server.createSession
    PDS-->>CLI: { did, handle, accessJwt, refreshJwt }

    CLI->>CLI: Save account config + session tokens
    CLI->>CLI: Load or generate X25519 keypair

    CLI->>PDS: com.atproto.repo.putRecord (publicKey/self)
    PDS-->>CLI: { uri, cid }

    CLI->>User: Logged in as <handle>
```

The `putRecord` call is idempotent — same key, same record. Safe to call on every login.

## Token Refresh

Transparent to the user. The XRPC client detects expired tokens and refreshes automatically. The refresh path depends on the session variant.

### Legacy Refresh

```mermaid
sequenceDiagram
    participant CLI
    participant PDS

    CLI->>PDS: Any XRPC call (expired accessJwt)
    PDS-->>CLI: 400 ExpiredToken

    CLI->>PDS: com.atproto.server.refreshSession (refreshJwt)
    PDS-->>CLI: { accessJwt, refreshJwt } (new tokens)

    CLI->>CLI: Update stored session

    CLI->>PDS: Retry original XRPC call (new accessJwt)
    PDS-->>CLI: Success
```

### OAuth Refresh

```mermaid
sequenceDiagram
    participant CLI
    participant AS

    CLI->>AS: Any XRPC call (expired access_token, DPoP proof)
    AS-->>CLI: 400 ExpiredToken

    CLI->>AS: POST /oauth/token (grant_type=refresh_token, DPoP proof)
    AS-->>CLI: { access_token, refresh_token } (new tokens)

    CLI->>CLI: Update stored OAuthSession

    CLI->>AS: Retry original XRPC call (new access_token, DPoP proof)
    AS-->>CLI: Success
```

DPoP nonces are captured from every response and included in subsequent proofs. If the server returns a `use_dpop_nonce` error, the client retries once with the fresh nonce.
