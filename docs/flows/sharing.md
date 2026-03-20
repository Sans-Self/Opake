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

    CLI->>PDS: createRecord (grant)
    PDS-->>CLI: { uri }
    CLI->>PDS: deleteRecord (pendingShare)
    PDS-->>CLI: 200 OK
```

Pending shares expire after 7 days. The daemon also deletes expired records on each pass.
