# Authentication

## Login

Authenticates with a PDS, persists session + identity, and publishes the encryption public key as a singleton record.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS

    User->>CLI: opake login --pds <url> --identifier <handle>
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

Transparent to the user. The XRPC client detects expired tokens and refreshes automatically.

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
