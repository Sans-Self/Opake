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
    TargetPDS-->>CLI: PublicKeyRecord { x25519 + ml_kem halves, algos }

    CLI->>User: DID, handle, PDS URL, hybrid public keys, algorithms
```

Resolution rejects a public-key record whose declared algorithm is not `x25519` / `ml-kem-768` before decoding any key bytes, so a corrupt or mislabelled key fails here with a clear reason rather than deep inside a wrap call (`spec:sharing-grants § The recipient's keys are discovered from their published public-key record`).

## Share

Grants another user access to a document by wrapping the content key to their public key. Sharing is **cabinet-only**: a share hands out one document's content key to one recipient as a standalone grant record. Workspace documents are reached through group keys, not grants, so `share` from a workspace context is refused (`spec:sharing-grants § Sharing is cabinet-only`).

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

A recipient who exists but has not published a `publicKey/self` record cannot receive a share yet. Resolution distinguishes this case — a valid DID with no key surfaces as `RecipientNotReady`, not `NotFound`, so a typo'd handle still fails outright. The client does **not** queue automatically: it warns that the recipient exists but has not set up Opake, and queues a `pendingShare` only on the user's explicit confirmation (`spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped`). The pending record carries the document, the recipient as entered, and the grant metadata encrypted under the document's content key — no plaintext, and enough to reconstruct the grant later.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS as Own PDS
    participant PLC as PLC Directory
    participant RecipientPDS as Recipient's PDS

    User->>CLI: opake share photo.jpg bob.test

    CLI->>PLC: DID document for recipient
    PLC-->>CLI: { pds_url }
    CLI->>RecipientPDS: getRecord (publicKey/self)
    RecipientPDS-->>CLI: 404 → RecipientNotReady

    CLI->>User: bob.test exists but hasn't set up Opake. Queue the share? [y/N]
    User-->>CLI: y

    CLI->>CLI: fetch document content key, encrypt grant metadata under it
    CLI->>PDS: createRecord (pendingShare)
    PDS-->>CLI: { uri }
    CLI->>User: Share queued — will complete when they publish a key

    Note over CLI: Later, daemon tick...
    CLI->>PDS: listRecords (pendingShare)
    PDS-->>CLI: [ pendingShare record ]

    CLI->>RecipientPDS: getRecord (publicKey/self)
    RecipientPDS-->>CLI: PublicKeyRecord (now published)

    Note over CLI: Recipient is ready — complete the share
    CLI->>PDS: getRecord (document) → unwrap content key, wrap to recipient
    CLI->>PDS: putRecord (grant @ pendingShare rkey)
    CLI->>PDS: deleteRecord (pendingShare)
```

Completion writes the grant at the **pending share's own rkey** via an idempotent `putRecord`, not at a fresh rkey via `createRecord`. That is what keeps the retry runner exactly-once when more than one runner is live — a daemon and an open tab, say. Both derive the same grant rkey from the same pending record and upsert there, so the repo converges on one grant instead of one per runner. The `deleteRecord` that follows is idempotent cleanup: a runner that finds the pending record already gone treats that as done. See [Background maintenance](../FLOWS.md#background-maintenance--multi-runner-coordination) for the full race walkthrough and [docs/BACKGROUND_WORK.md](../BACKGROUND_WORK.md) for the contract.

Pending shares expire after 7 days. The daemon also deletes expired records on each pass.
