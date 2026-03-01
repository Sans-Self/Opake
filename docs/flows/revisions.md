# Revisions (Planned)

Collaborative editing via revision records. Each member uploads revisions to their own PDS — data stays under their control, and the AppView stitches it together. Same pattern as Bluesky replies: your content lives on your PDS, the AppView presents the thread.

## Propose Revision (Direct Share)

A grant recipient uploads a revised version of a shared document to their own PDS.

```mermaid
sequenceDiagram
    participant Recipient
    participant CLI as Recipient's CLI
    participant Crypto
    participant RecipientPDS as Recipient's PDS

    Recipient->>CLI: opake propose at://owner/.../document/tid photo-edited.jpg

    CLI->>CLI: Read new file, detect MIME type
    CLI->>Crypto: generate_content_key() → K'
    CLI->>Crypto: encrypt_blob(K', plaintext)
    Crypto-->>CLI: { ciphertext, nonce }

    CLI->>RecipientPDS: uploadBlob (ciphertext)
    RecipientPDS-->>CLI: blob ref

    CLI->>Crypto: wrap_key(K', recipient_own_pubkey)
    Crypto-->>CLI: wrappedKey (self-wrap)

    CLI->>RecipientPDS: createRecord (revision)
    Note right of RecipientPDS: origin: at://owner/.../document/tid<br/>blob, encryption, nonce
    RecipientPDS-->>CLI: { uri, cid }

    CLI->>Recipient: Proposed: at://recipient/.../revision/tid
```

The revision record lives on the recipient's PDS. It references the original document via an `origin` AT-URI. The owner's data is untouched.

## Propose Revision (Keyring Member)

A keyring member uploads a revised version, encrypted under the shared group key.

```mermaid
sequenceDiagram
    participant Member
    participant CLI as Member's CLI
    participant Crypto
    participant MemberPDS as Member's PDS
    participant Disk as Local Storage

    Member->>CLI: opake propose at://owner/.../document/tid recipe-v2.pdf --keyring family

    CLI->>Disk: Load group key GK for keyring
    Disk-->>CLI: GK

    CLI->>CLI: Read new file, detect MIME type
    CLI->>Crypto: generate_content_key() → K'
    CLI->>Crypto: encrypt_blob(K', plaintext)
    Crypto-->>CLI: { ciphertext, nonce }

    CLI->>MemberPDS: uploadBlob (ciphertext)
    MemberPDS-->>CLI: blob ref

    CLI->>Crypto: wrap_content_key_for_keyring(K', GK)
    Crypto-->>CLI: AES-KW wrapped content key

    CLI->>MemberPDS: createRecord (revision with keyringEncryption)
    Note right of MemberPDS: origin: at://owner/.../document/tid<br/>keyring ref from original document
    MemberPDS-->>CLI: { uri, cid }

    CLI->>Member: Proposed: at://member/.../revision/tid
```

Any keyring member can decrypt the revision — they already have GK.

## Accept Revision (Owner)

The document owner reviews a proposed revision and applies it by replacing their blob.

```mermaid
sequenceDiagram
    participant Owner
    participant CLI as Owner's CLI
    participant PLC as PLC Directory
    participant ProposerPDS as Proposer's PDS
    participant Crypto
    participant OwnerPDS as Owner's PDS

    Owner->>CLI: opake accept at://proposer/.../revision/tid

    CLI->>CLI: Parse revision URI, extract proposer DID
    CLI->>PLC: DID document for proposer
    PLC-->>CLI: { pds_url }

    CLI->>ProposerPDS: getRecord (revision)
    ProposerPDS-->>CLI: Revision record { origin, blob, encryption }

    CLI->>Crypto: Decrypt revision blob (via grant key or GK)
    Crypto-->>CLI: new plaintext

    Note over CLI: Re-encrypt under owner's own keys
    CLI->>Crypto: generate_content_key() → K''
    CLI->>Crypto: encrypt_blob(K'', plaintext)
    Crypto-->>CLI: { ciphertext, nonce }

    CLI->>OwnerPDS: uploadBlob (ciphertext)
    OwnerPDS-->>CLI: new blob ref

    CLI->>Crypto: Wrap K'' (to owner + keyring/grants as before)
    Crypto-->>CLI: new wrapped keys

    CLI->>OwnerPDS: putRecord (update document with new blob + keys)
    OwnerPDS-->>CLI: 200 OK

    CLI->>Owner: Accepted revision, document updated
```

The owner re-encrypts with a fresh content key rather than reusing the proposer's. This ensures the owner's document record remains self-consistent — all wrapped keys reference the same content key, and the blob is stored on the owner's PDS.

## Discovery via AppView

Without an AppView, revision discovery requires polling known members' PDSes. The AppView automates this by watching firehose events.

```mermaid
sequenceDiagram
    participant AppView
    participant MemberPDS as Member's PDS
    participant OwnerPDS as Owner's PDS

    MemberPDS->>AppView: Firehose event: new revision record
    AppView->>AppView: Index revision by origin URI

    Note over AppView: Later, owner queries pending revisions

    OwnerPDS->>AppView: GET /revisions?origin=at://owner/.../document/tid
    AppView-->>OwnerPDS: [{ revision_uri, proposer, created_at }, ...]
```

Without the AppView, `opake revisions <document>` can fall back to polling each keyring member's PDS for `app.opake.cloud.revision` records whose `origin` matches the document URI. Slow but functional, and zero-trust — no intermediary needed.
