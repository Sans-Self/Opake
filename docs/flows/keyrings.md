# Keyrings

## Keyring Model

Two-layer key wrapping for group access. Per-document content keys are wrapped under the group key (AES-KW), and the group key is wrapped to each member's X25519 public key.

```mermaid
flowchart TB
    subgraph Keyring ["Keyring Record"]
        GK["Group Key GK"]
        GK -->|wrapped to| Alice["Alice's pubkey"]
        GK -->|wrapped to| Bob["Bob's pubkey"]
        GK -->|wrapped to| Carol["Carol's pubkey"]
    end

    subgraph Doc1 ["Document 1"]
        K1["Content Key K₁"] -->|wrapped under GK| GK
    end

    subgraph Doc2 ["Document 2"]
        K2["Content Key K₂"] -->|wrapped under GK| GK
    end

    Alice -.->|unwrap GK, then K₁| K1
    Bob -.->|unwrap GK, then K₂| K2

    style Keyring fill:#1a1a2e,color:#eee
    style Doc1 fill:#16213e,color:#eee
    style Doc2 fill:#16213e,color:#eee
```

## Create Keyring

Generates a group key, wraps it to the owner, creates the keyring record, and stores the group key locally.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Crypto
    participant PDS
    participant Disk as Local Storage

    User->>CLI: opake keyring create family-photos

    CLI->>Crypto: create_group_key()
    Crypto-->>CLI: group key GK + wrappedKey (GK → owner pubkey)

    CLI->>PDS: com.atproto.repo.createRecord (keyring)
    PDS-->>CLI: { uri, cid }

    CLI->>Disk: Save GK to ~/.config/opake/accounts/<did>/keyrings/<rkey>.json

    CLI->>User: family-photos → at://did/.../keyring-tid
```

The group key is never stored in plaintext on the PDS — only the wrapped copies live in the keyring record.

## List Keyrings

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS

    User->>CLI: opake keyring ls --long

    loop Paginate until no cursor
        CLI->>PDS: com.atproto.repo.listRecords (keyring collection, cursor)
        PDS-->>CLI: { records: [...], cursor? }
    end

    CLI->>User: Display table (name, members, rotation, URI)
```

## Add Member

Resolves the new member's identity, wraps the group key to their public key, and appends them to the keyring record.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS as Own PDS
    participant PLC as PLC Directory
    participant MemberPDS as Member's PDS
    participant Crypto
    participant Disk as Local Storage

    User->>CLI: opake keyring add-member family-photos alice.example.com

    CLI->>PDS: listRecords → resolve "family-photos" to keyring URI
    PDS-->>CLI: keyring URI + rkey

    CLI->>Disk: Load group key GK for this keyring
    Disk-->>CLI: GK

    Note over CLI,MemberPDS: Resolve new member identity
    CLI->>PLC: DID document for alice
    PLC-->>CLI: { pds_url }
    CLI->>MemberPDS: getRecord (publicKey/self)
    MemberPDS-->>CLI: Alice's X25519 public key

    CLI->>Crypto: wrap_key(GK, alice_pubkey, alice_did)
    Crypto-->>CLI: wrappedKey for Alice

    CLI->>PDS: getRecord (keyring) → append Alice → putRecord
    PDS-->>CLI: 200 OK

    CLI->>User: added alice.example.com to family-photos
```

## Remove Member

Removes the member, generates a new group key, re-wraps to all remaining members, and increments the rotation counter.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS as Own PDS
    participant PLC as PLC Directory
    participant Crypto
    participant Disk as Local Storage

    User->>CLI: opake keyring remove-member family-photos bob.example.com

    CLI->>PDS: Resolve keyring URI + fetch keyring record
    PDS-->>CLI: Keyring with members [Alice, Bob, Carol]

    CLI->>User: Removing bob will rotate the group key. Continue? [y/N]
    User-->>CLI: y

    Note over CLI,PLC: Resolve remaining members' public keys
    CLI->>PLC: DID documents for Alice, Carol
    PLC-->>CLI: PDS URLs
    CLI->>PDS: getRecord (publicKey/self) for each
    PDS-->>CLI: Public keys for Alice, Carol

    CLI->>Crypto: create_group_key() → new GK'
    Crypto-->>CLI: GK' + wrappedKeys for [Alice, Carol]

    CLI->>PDS: putRecord (keyring: members=[Alice, Carol], rotation++, keyHistory appended)
    PDS-->>CLI: 200 OK

    CLI->>Disk: Save new GK' alongside old GK (keyed by rotation)

    CLI->>User: removed bob from family-photos (key rotated)
```

Before replacing the group key, the old rotation's remaining member entries are archived into the keyring's `keyHistory` array. This lets remaining members still decrypt documents uploaded under previous rotations — even on a new device, the old wrapped group keys are preserved in the record.

Existing documents encrypted under the old group key stay as-is. New uploads use the new group key. Removed members' wrapped keys are excluded from history, so they cannot recover old group keys from the record.

## Upload with Keyring

Encrypts a file and wraps the content key under the keyring's group key instead of individual public keys.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Crypto
    participant PDS
    participant Disk as Local Storage

    User->>CLI: opake upload photo.jpg --keyring family-photos

    CLI->>PDS: listRecords → resolve "family-photos" to keyring URI
    PDS-->>CLI: keyring URI + rkey + rotation

    CLI->>Disk: Load group key GK
    Disk-->>CLI: GK

    CLI->>CLI: Read file from disk, detect MIME type
    CLI->>Crypto: generate_content_key() → K
    CLI->>Crypto: encrypt_blob(K, plaintext)
    Crypto-->>CLI: { ciphertext, nonce }

    CLI->>PDS: com.atproto.repo.uploadBlob (ciphertext)
    PDS-->>CLI: blob ref

    CLI->>Crypto: wrap_content_key_for_keyring(K, GK)
    Crypto-->>CLI: AES-KW wrapped content key

    CLI->>PDS: createRecord (document with keyringEncryption)
    PDS-->>CLI: { uri, cid }

    CLI->>User: Uploaded: at://did/.../document-tid
```

The document record references the keyring URI and stores `wrappedContentKey` (content key wrapped under GK) instead of per-DID wrapped keys.

## Download Keyring-Encrypted Document

Automatically detected — the CLI peeks at the document's encryption type and loads the group key if needed.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS
    participant Crypto
    participant Disk as Local Storage

    User->>CLI: opake download photo.jpg

    CLI->>CLI: Resolve filename → AT-URI

    CLI->>PDS: com.atproto.repo.getRecord (document)
    PDS-->>CLI: Document record (keyringEncryption variant)

    CLI->>CLI: Detect keyring encryption, extract keyring rkey + rotation

    CLI->>Disk: Load group key GK for this keyring at document's rotation
    Disk-->>CLI: GK

    CLI->>Crypto: unwrap_content_key_from_keyring(wrappedContentKey, GK)
    Crypto-->>CLI: content key K

    CLI->>PDS: com.atproto.sync.getBlob (did, cid)
    PDS-->>CLI: ciphertext bytes

    CLI->>Crypto: decrypt_blob(K, nonce, ciphertext)
    Crypto-->>CLI: plaintext

    CLI->>CLI: Write plaintext to disk
    CLI->>User: Saved to ./photo.jpg
```
