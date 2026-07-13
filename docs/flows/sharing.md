# Sharing

## Resolve

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

## Share

Grants another user access to a document by wrapping the content key to their public key.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Opake as Opake + FileManager
    participant PDS as Own PDS
    participant PLC as PLC Directory
    participant RecipientPDS as Recipient's PDS
    participant Crypto

    User->>CLI: opake share photo.jpg alice.example.com

    CLI->>Opake: ctx.opake() + file_context(None) + file_manager(&ctx)
    Opake->>Opake: mgr.resolve_entry(&tree, "photo.jpg")

    Note over Opake,RecipientPDS: Resolve recipient identity
    Opake->>PLC: DID document for recipient
    PLC-->>Opake: { pds_url }
    Opake->>RecipientPDS: getRecord (publicKey/self)
    RecipientPDS-->>Opake: recipient's X25519 public key

    Note over Opake,PDS: Fetch content key from own document
    Opake->>PDS: getRecord (document)
    PDS-->>Opake: Document record with owner's wrappedKey
    Opake->>Crypto: unwrap_key(owner_wrappedKey, private_key)
    Crypto-->>Opake: content key K

    Note over Opake,PDS: Create grant
    Opake->>Crypto: wrap_key(K, recipient_pubkey, recipient_did)
    Crypto-->>Opake: wrappedKey for recipient

    Opake->>PDS: createRecord (grant)
    PDS-->>Opake: { uri, cid }

    Note over Opake: #[signoff] auto-persists session if refreshed

    CLI->>User: Shared: at://did/.../grant-tid
```

## Revoke

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

## Pending Share (recipient not ready)

When the recipient hasn't set up Opake yet (no `publicKey/self`), the share is queued as a `pendingShare` record on the PDS. The daemon retries periodically.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS as Own PDS
    participant PLC as PLC Directory
    participant RecipientPDS as Recipient's PDS

    User->>CLI: opake share photo.jpg bob.test

    CLI->>CLI: Resolve filename → AT-URI
    CLI->>PLC: DID document for recipient
    PLC-->>CLI: { pds_url }
    CLI->>RecipientPDS: getRecord (publicKey/self)
    RecipientPDS-->>CLI: 404 Not Found

    Note over CLI,PDS: Recipient hasn't set up Opake — queue for retry
    CLI->>PDS: createRecord (pendingShare)
    PDS-->>CLI: { uri }
    CLI->>User: Share queued — will complete when they log in

    Note over CLI: Later, daemon tick...
    CLI->>PDS: listRecords (pendingShare)
    PDS-->>CLI: [ pendingShare record ]

    CLI->>PLC: DID document for recipient
    PLC-->>CLI: { pds_url }
    CLI->>RecipientPDS: getRecord (publicKey/self)
    RecipientPDS-->>CLI: PublicKeyRecord { publicKey }

    Note over CLI: Recipient is ready — complete the share
    CLI->>PDS: getRecord (document)
    PDS-->>CLI: Document with owner's wrappedKey
    CLI->>CLI: unwrap content key, wrap to recipient

    CLI->>PDS: putRecord (grant @ pendingShare rkey)
    PDS-->>CLI: { uri }
    CLI->>PDS: deleteRecord (pendingShare)
    PDS-->>CLI: 200 OK
```

Completion writes the grant at the **pending share's own rkey** via an idempotent `putRecord`, not at a fresh rkey via `createRecord`. That is what keeps the retry runner exactly-once when more than one runner is live — a daemon and an open tab, say. Both derive the same grant rkey from the same pending record and upsert there, so the repo converges on one grant instead of one per runner. The `deleteRecord` that follows is idempotent cleanup: a runner that finds the pending record already gone treats that as done. See [Background maintenance](../FLOWS.md#background-maintenance--multi-runner-coordination) for the full race walkthrough and [docs/BACKGROUND_WORK.md](../BACKGROUND_WORK.md) for the contract.

Pending shares expire after 7 days. The daemon also deletes expired records on each pass.
