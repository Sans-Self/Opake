# Opake — Operation Flows

Sequence diagrams for every CLI operation. All crypto happens client-side — the PDS only stores and serves opaque bytes.

## Authentication

### Login

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

### Token Refresh

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

## Document Operations

### Upload

Encrypts a file and uploads it as an opaque blob with a metadata record.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Crypto
    participant PDS

    User->>CLI: opake upload photo.jpg --tags vacation

    CLI->>CLI: Read file from disk, detect MIME type
    CLI->>Crypto: generate_content_key()
    Crypto-->>CLI: random AES-256-GCM key K

    CLI->>Crypto: encrypt_blob(K, plaintext)
    Crypto-->>CLI: { ciphertext, nonce }

    CLI->>PDS: com.atproto.repo.uploadBlob (ciphertext)
    PDS-->>CLI: blob ref { $link, size }

    CLI->>Crypto: wrap_key(K, owner_pubkey, owner_did)
    Crypto-->>CLI: wrappedKey (x25519-hkdf-a256kw)

    CLI->>PDS: com.atproto.repo.createRecord (document)
    PDS-->>CLI: { uri, cid }

    CLI->>User: Uploaded: at://did/app.opake.cloud.document/<tid>
```

### Download (Own Files)

Fetches a document you own, unwraps the content key, and decrypts.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS
    participant Crypto

    User->>CLI: opake download photo.jpg

    CLI->>CLI: Resolve filename → AT-URI (via listRecords if needed)

    CLI->>PDS: com.atproto.repo.getRecord (document)
    PDS-->>CLI: Document record (envelope, blob ref)

    CLI->>CLI: Find wrappedKey matching own DID
    CLI->>Crypto: unwrap_key(wrappedKey, private_key)
    Crypto-->>CLI: content key K

    CLI->>PDS: com.atproto.sync.getBlob (did, cid)
    PDS-->>CLI: ciphertext bytes

    CLI->>Crypto: decrypt_blob(K, nonce, ciphertext)
    Crypto-->>CLI: plaintext

    CLI->>CLI: Write plaintext to disk
    CLI->>User: Saved to ./photo.jpg
```

### Download (Shared Files — Cross-PDS)

Downloads a file shared with you by another user. Requires the grant URI (auto-discovery via `inbox` is not yet implemented).

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PLC as PLC Directory
    participant OwnerPDS as Owner's PDS
    participant Crypto

    User->>CLI: opake download --grant at://did:plc:owner/.../grant-tid

    CLI->>CLI: Parse grant URI, extract owner DID

    CLI->>PLC: GET /did:plc:owner (DID document)
    PLC-->>CLI: { service: [{ #atproto_pds: owner-pds-url }] }

    CLI->>OwnerPDS: com.atproto.repo.getRecord (grant)
    OwnerPDS-->>CLI: Grant record { document, wrappedKey }

    CLI->>Crypto: unwrap_key(grant.wrappedKey, private_key)
    Crypto-->>CLI: content key K

    CLI->>OwnerPDS: com.atproto.repo.getRecord (document)
    OwnerPDS-->>CLI: Document record { blob, encryption.nonce }

    CLI->>OwnerPDS: com.atproto.sync.getBlob (did, cid)
    OwnerPDS-->>CLI: ciphertext bytes

    CLI->>Crypto: decrypt_blob(K, nonce, ciphertext)
    Crypto-->>CLI: plaintext

    CLI->>CLI: Write to disk
    CLI->>User: Saved to ./shared-file.txt
```

Data never leaves the owner's PDS. The recipient fetches everything directly from the source.

### List

Lists document records on your PDS with optional tag filtering.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS

    User->>CLI: opake ls --tag vacation --long

    loop Paginate until no cursor
        CLI->>PDS: com.atproto.repo.listRecords (collection, cursor)
        PDS-->>CLI: { records: [...], cursor? }
    end

    CLI->>CLI: Parse documents, filter by tag
    CLI->>User: Display table (name, size, tags, URI)
```

### Delete

Deletes a document record. The blob becomes orphaned and is eventually garbage-collected by the PDS.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS

    User->>CLI: opake rm photo.jpg

    CLI->>CLI: Resolve filename → AT-URI
    CLI->>User: Delete photo.jpg? [y/N]
    User-->>CLI: y

    CLI->>PDS: com.atproto.repo.deleteRecord (collection, rkey)
    PDS-->>CLI: 200 OK

    CLI->>User: Deleted
```

## Sharing

### Resolve

Resolves a handle or DID to its PDS and X25519 public key. Used internally by `share`, exposed as a standalone command for inspection.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant CallerPDS as Caller's PDS
    participant PLC as PLC Directory
    participant TargetPDS as Target's PDS

    User->>CLI: opake resolve alice.example.com

    alt Input is a handle
        CLI->>CallerPDS: com.atproto.identity.resolveHandle
        CallerPDS-->>CLI: did:plc:alice
    else Input is a DID
        CLI->>CLI: Use directly
    end

    CLI->>PLC: GET /did:plc:alice (DID document)
    PLC-->>CLI: { alsoKnownAs, service: [#atproto_pds → pds-url] }

    CLI->>TargetPDS: com.atproto.repo.getRecord (publicKey/self)
    TargetPDS-->>CLI: PublicKeyRecord { publicKey, algo }

    CLI->>User: DID, handle, PDS URL, public key, algorithm
```

### Share

Grants another user access to a document by wrapping the content key to their public key.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS as Own PDS
    participant PLC as PLC Directory
    participant RecipientPDS as Recipient's PDS
    participant Crypto

    User->>CLI: opake share photo.jpg alice.example.com

    CLI->>CLI: Resolve filename → AT-URI

    Note over CLI,RecipientPDS: Resolve recipient identity
    CLI->>PLC: DID document for recipient
    PLC-->>CLI: { pds_url }
    CLI->>RecipientPDS: getRecord (publicKey/self)
    RecipientPDS-->>CLI: recipient's X25519 public key

    Note over CLI,PDS: Fetch content key from own document
    CLI->>PDS: getRecord (document)
    PDS-->>CLI: Document record with owner's wrappedKey
    CLI->>Crypto: unwrap_key(owner_wrappedKey, private_key)
    Crypto-->>CLI: content key K

    Note over CLI,PDS: Create grant
    CLI->>Crypto: wrap_key(K, recipient_pubkey, recipient_did)
    Crypto-->>CLI: wrappedKey for recipient

    CLI->>PDS: createRecord (grant)
    PDS-->>CLI: { uri, cid }

    CLI->>User: Shared: at://did/.../grant-tid
```

### Revoke

Deletes a grant record. The recipient loses network access to the wrapped key.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS

    User->>CLI: opake revoke at://did/.../grant-tid

    CLI->>CLI: Validate URI is a grant collection
    CLI->>PDS: com.atproto.repo.deleteRecord (grant collection, rkey)
    PDS-->>CLI: 200 OK

    CLI->>User: Revoked
```

For true forward secrecy, the document should also be re-encrypted with a new content key — the schema supports this but the CLI doesn't automate it yet.

## Encryption Primitives

### Key Wrapping (x25519-hkdf-a256kw)

How a symmetric content key gets wrapped to a recipient's X25519 public key. This is the core crypto operation behind both direct encryption and grant creation.

```mermaid
flowchart LR
    subgraph Wrap ["wrap_key()"]
        direction TB
        EphKey["Generate ephemeral<br/>X25519 keypair"] --> ECDH
        RecipPub["Recipient's<br/>X25519 public key"] --> ECDH
        ECDH["X25519 ECDH<br/>shared secret"] --> HKDF
        HKDF["HKDF-SHA256<br/>info = 'opake-v1-x25519-hkdf-a256kw-{did}'"] --> KEK
        KEK["256-bit key<br/>encryption key"] --> AESKW
        ContentKey["Content key K<br/>(AES-256)"] --> AESKW
        AESKW["AES-256-KW"] --> Ciphertext
    end

    Ciphertext["wrappedKey.ciphertext:<br/>[32B ephemeral pubkey ‖ 40B wrapped key]"]

    style Wrap fill:#1a1a2e,color:#eee
    style Ciphertext fill:#16213e,color:#eee
```

### Content Encryption (AES-256-GCM)

```mermaid
flowchart LR
    subgraph Encrypt ["encrypt_blob()"]
        direction TB
        K["Content key K"] --> GCM
        Nonce["Random 12-byte nonce"] --> GCM
        Plaintext["File bytes"] --> GCM
        GCM["AES-256-GCM"]
    end

    GCM --> Ciphertext["Ciphertext + auth tag"]
    GCM --> StoredNonce["Nonce stored in<br/>document record"]

    style Encrypt fill:#1a1a2e,color:#eee
```

## Keyring Sharing (Planned)

Not yet implemented. This is the two-layer key model for group access.

```mermaid
flowchart TB
    subgraph Keyring ["Keyring Record"]
        GK["Group Key GK"]
        GK -->|wrapped to| Alice["Alice's pubkey"]
        GK -->|wrapped to| Bob["Bob's pubkey"]
        GK -->|wrapped to| Carol["Carol's pubkey"]
    end

    subgraph Doc1 ["Document 1"]
        K1["Content Key K₁"] -->|wrapped under| GK
    end

    subgraph Doc2 ["Document 2"]
        K2["Content Key K₂"] -->|wrapped under| GK
    end

    Alice -.->|unwrap GK, then K₁| K1
    Bob -.->|unwrap GK, then K₂| K2

    style Keyring fill:#1a1a2e,color:#eee
    style Doc1 fill:#16213e,color:#eee
    style Doc2 fill:#16213e,color:#eee
```

Adding a member wraps GK to their public key. Removing a member rotates GK and re-wraps to the remaining members. Per-document content keys and blobs stay untouched.
