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

Generates a group key, wraps it to the owner (with `role: manager`), creates the keyring record with the `owner` field set to the creator's DID, and stores the group key locally.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Crypto
    participant PDS
    participant Disk as Local Storage

    User->>CLI: opake workspace create family-photos

    CLI->>Crypto: create_group_key()
    Crypto-->>CLI: group key GK + wrappedKey (GK → owner pubkey, role=manager)

    CLI->>PDS: com.atproto.repo.createRecord (keyring, owner=self DID)
    PDS-->>CLI: { uri, cid }

    CLI->>Disk: Save GK to ~/.config/opake/accounts/<did>/keyrings/<rkey>.json

    CLI->>User: family-photos → at://did/.../keyring-tid
```

The group key is never stored in plaintext on the PDS — only the wrapped copies live in the keyring record. The `owner` field identifies the canonical keyring owner for Indexer authorization.

## List Keyrings

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS

    User->>CLI: opake workspace ls --long

    loop Paginate until no cursor
        CLI->>PDS: com.atproto.repo.listRecords (keyring collection, cursor)
        PDS-->>CLI: { records: [...], cursor? }
    end

    CLI->>User: Display table (name, members, rotation, URI)
```

## Add Member

Resolves the new member's identity, wraps the group key to their public key with the specified role, and appends them to the keyring record.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS as Own PDS
    participant PLC as PLC Directory
    participant MemberPDS as Member's PDS
    participant Crypto
    participant Disk as Local Storage

    User->>CLI: opake workspace add-member family-photos alice.example.com --role editor

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
    Crypto-->>CLI: wrappedKey for Alice (role=editor)

    CLI->>PDS: getRecord (keyring) → append Alice with role → putRecord
    PDS-->>CLI: 200 OK

    CLI->>User: added alice.example.com to family-photos (editor)
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

    User->>CLI: opake workspace remove-member family-photos bob.example.com

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

## Upload with Workspace

Workspace uploads split by caller role, but both write the canonical `app.opake.document` record to the **caller's** PDS — federated, atproto-shaped. The keyring (which holds the group key wrapped to each member) and the directory record (which holds the entry list) stay on the workspace owner's PDS.

- **Owner uploads** are atomic on the owner's PDS: blob + document + directory entry update in a single `applyWrites`.
- **Member uploads** are atomic on the member's PDS: blob + document + a `directoryUpdate.addEntry` proposal in a single `applyWrites`. The owner's daemon applies the proposal to register the entry in the workspace directory.

The encryption surface is identical in both cases: a per-document content key, AES-256-GCM blob, content key wrapped under the workspace group key (AES-KW). The difference is only the second write op (directory entry vs. directoryUpdate proposal).

```mermaid
sequenceDiagram
    participant User
    participant Web as Caller's Browser
    participant Opake as Opake + FileManager
    participant Crypto
    participant CallerPDS as Caller's PDS

    User->>Web: Upload photo.jpg to family-photos

    Web->>Opake: ctx.opake() + file_context(Some("family-photos"))
    Note over Opake: resolve_workspace: keyring lookup + group key unwrap

    Opake->>Crypto: generate_content_key() → K
    Opake->>Crypto: encrypt_blob(K, plaintext)
    Crypto-->>Opake: { ciphertext, nonce }

    Opake->>CallerPDS: com.atproto.repo.uploadBlob (ciphertext)
    CallerPDS-->>Opake: blob ref

    Opake->>Crypto: wrap_content_key_for_keyring(K, GK)
    Crypto-->>Opake: AES-KW wrapped content key

    alt Caller is workspace owner
        Opake->>CallerPDS: applyWrites (createDocument + updateDirectory)
        Note right of CallerPDS: Document and directory both on owner's PDS<br/>Directory entries updated atomically
        CallerPDS-->>Opake: { uri, cid }
        Opake->>User: Uploaded
    else Caller is workspace member
        Opake->>CallerPDS: applyWrites (createDocument + createDirectoryUpdate)
        Note right of CallerPDS: Document on member's PDS<br/>directoryUpdate.addEntry proposal alongside it
        CallerPDS-->>Opake: { uri, cid }
        Opake->>User: Uploaded — pending owner review
    end
```

The directory entry list on the owner's PDS holds at-URIs that may resolve to records on any member's PDS. Reads federate: the indexer (or a client doing public XRPC) walks `entries` and fetches each document from whichever PDS hosts it. Storage and egress costs land on the contributor that wrote the file, not the workspace owner.

After the owner's daemon applies the `directoryUpdate.addEntry` proposal, the directory's `modifiedAt` advances and the editor's cleanup module deletes the proposal record (see [revisions.md](revisions.md#editor-side-cleanup)).

## Download Workspace Document

Automatically detected -- the FileManager peeks at the document's encryption type and uses the workspace's group key.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Opake as Opake + FileManager
    participant PDS
    participant Crypto

    User->>CLI: opake download photo.jpg --workspace family-photos

    CLI->>Opake: ctx.opake() + file_context(Some("family-photos")) + file_manager(&ctx)
    Opake->>Opake: mgr.download_at("photo.jpg")

    Opake->>PDS: com.atproto.repo.getRecord (document)
    PDS-->>Opake: Document record (keyringEncryption variant)

    Opake->>Opake: Detect keyring encryption, use workspace group key GK

    Opake->>Crypto: unwrap_content_key_from_keyring(wrappedContentKey, GK)
    Crypto-->>Opake: content key K

    Opake->>PDS: com.atproto.sync.getBlob (did, cid)
    PDS-->>Opake: ciphertext bytes

    Opake->>Crypto: decrypt_blob(K, nonce, ciphertext)
    Crypto-->>Opake: plaintext

    Note over Opake: #[signoff] auto-persists session if refreshed

    CLI->>CLI: Write plaintext to disk
    CLI->>User: Saved to ./photo.jpg
```
