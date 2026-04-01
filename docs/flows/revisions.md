# Document Updates (Collaborative Editing)

Collaborative editing via `app.opake.documentUpdate` records. Each editor uploads updates to their own PDS — data stays under their control, and the AppView surfaces pending updates to the document owner. Same pattern as Bluesky replies: your content lives on your PDS, the AppView presents the thread.

## Propose Update (Workspace Editor)

A workspace editor uploads a revised version of a document, encrypted under the shared group key.

```mermaid
sequenceDiagram
    participant Editor
    participant CLI as Editor's CLI
    participant Crypto
    participant EditorPDS as Editor's PDS
    participant Disk as Local Storage

    Editor->>CLI: opake update at://owner/.../document/tid recipe-v2.pdf

    CLI->>Disk: Load group key GK for keyring
    Disk-->>CLI: GK

    CLI->>CLI: Read new file, detect MIME type
    CLI->>Crypto: generate_content_key() → K'
    CLI->>Crypto: encrypt_blob(K', plaintext)
    Crypto-->>CLI: { ciphertext, nonce }

    CLI->>EditorPDS: uploadBlob (ciphertext)
    EditorPDS-->>CLI: blob ref

    CLI->>Crypto: wrap_content_key_for_keyring(K', GK)
    Crypto-->>CLI: AES-KW wrapped content key

    CLI->>EditorPDS: createRecord (documentUpdate with keyringEncryption)
    Note right of EditorPDS: document: at://owner/.../document/tid<br/>blob, encryptedMetadata
    EditorPDS-->>CLI: { uri, cid }

    CLI->>Editor: Proposed: at://editor/.../documentUpdate/tid
```

The update record lives on the editor's PDS. It references the target document via the `document` AT-URI. The owner's data is untouched until they apply it.

## Apply Update (Document Owner)

The document owner reviews a proposed update and applies it by replacing their blob.

```mermaid
sequenceDiagram
    participant Owner
    participant CLI as Owner's CLI
    participant PLC as PLC Directory
    participant EditorPDS as Editor's PDS
    participant Crypto
    participant OwnerPDS as Owner's PDS

    Owner->>CLI: opake apply at://editor/.../documentUpdate/tid

    CLI->>CLI: Parse update URI, extract editor DID
    CLI->>PLC: DID document for editor
    PLC-->>CLI: { pds_url }

    CLI->>EditorPDS: getRecord (documentUpdate)
    EditorPDS-->>CLI: Update record { document, blob, encryptedMetadata }

    CLI->>Crypto: Decrypt update blob (via GK)
    Crypto-->>CLI: new plaintext

    Note over CLI: Re-encrypt under owner's own keys
    CLI->>Crypto: generate_content_key() → K''
    CLI->>Crypto: encrypt_blob(K'', plaintext)
    Crypto-->>CLI: { ciphertext, nonce }

    CLI->>OwnerPDS: uploadBlob (ciphertext)
    OwnerPDS-->>CLI: new blob ref

    CLI->>Crypto: Wrap K'' (under keyring GK)
    Crypto-->>CLI: new wrapped content key

    CLI->>OwnerPDS: putRecord (update document with new blob + keys)
    OwnerPDS-->>CLI: 200 OK

    CLI->>Owner: Applied update, document updated
```

The owner re-encrypts with a fresh content key rather than reusing the editor's. This ensures the owner's document record remains self-consistent — all wrapped keys reference the same content key, and the blob is stored on the owner's PDS.

## Document Adoption

When a member is removed from a workspace, their documents need to be migrated. A remaining manager downloads, re-encrypts under the new group key, and uploads to their own PDS with a `supersedes` field for lineage.

```mermaid
sequenceDiagram
    participant Manager
    participant CLI as Manager's CLI
    participant AppView
    participant RemovedPDS as Removed Member's PDS
    participant Crypto
    participant ManagerPDS as Manager's PDS

    Manager->>AppView: GET /api/workspace?keyring={uri}
    AppView-->>Manager: documents including removed member's

    loop For each orphaned document
        Manager->>RemovedPDS: getRecord + getBlob
        RemovedPDS-->>Manager: document + ciphertext

        Manager->>Crypto: Decrypt with old GK (from keyHistory)
        Crypto-->>Manager: plaintext

        Manager->>Crypto: Re-encrypt with new GK
        Crypto-->>Manager: { ciphertext, nonce }

        Manager->>ManagerPDS: uploadBlob + createRecord (new document)
        ManagerPDS-->>Manager: { uri, cid }

        Manager->>ManagerPDS: createRecord (documentUpdate, supersedes=old URI)
    end

    Manager->>Manager: Adoption complete
```

Adoption must happen while the removed member's PDS is still serving data. The daemon should adopt eagerly on removal, not lazily.

## Discovery via AppView

The AppView watches firehose events for `documentUpdate` records and indexes them by target document.

```mermaid
sequenceDiagram
    participant AppView
    participant EditorPDS as Editor's PDS
    participant OwnerPDS as Owner's PDS

    EditorPDS->>AppView: Firehose event: new documentUpdate record
    AppView->>AppView: Validate editor role, index by document URI

    Note over AppView: Later, owner queries pending updates

    OwnerPDS->>AppView: GET /api/workspace/updates?document=at://owner/.../document/tid
    AppView-->>OwnerPDS: [{ update_uri, author_did, supersedes_uri, created_at }, ...]
```

Without the AppView, discovery falls back to polling each workspace member's PDS for `app.opake.documentUpdate` records whose `document` field matches. Slow but functional.
