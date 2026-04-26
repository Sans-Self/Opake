# Document Operations

## Upload

Encrypts a file and uploads it as an opaque blob with a metadata record.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Opake as Opake + FileManager
    participant Crypto
    participant PDS

    User->>CLI: opake upload photo.jpg

    CLI->>CLI: Read file from disk, detect MIME type
    CLI->>Opake: ctx.opake() + file_context(None) + file_manager(&ctx)

    Opake->>Crypto: generate_content_key()
    Crypto-->>Opake: random AES-256-GCM key K

    Opake->>Crypto: encrypt_blob(K, plaintext)
    Crypto-->>Opake: { ciphertext, nonce }

    Opake->>PDS: com.atproto.repo.uploadBlob (ciphertext)
    PDS-->>Opake: blob ref { $link, size }

    Opake->>Crypto: wrap_key(K, owner_pubkey, owner_did)
    Crypto-->>Opake: wrappedKey (x25519-mlkem768-hkdf-a256kw)

    Opake->>Crypto: encrypt_metadata(K, {name, mimeType, size, tags, ...})
    Crypto-->>Opake: encryptedMetadata { ciphertext, nonce }

    Opake->>PDS: com.atproto.repo.createRecord (document)
    PDS-->>Opake: { uri, cid }

    Note over Opake: #[signoff] auto-persists session if refreshed

    CLI->>User: Uploaded: at://did/app.opake.document/<tid>
```

## Download (Own Files)

Fetches a document you own, unwraps the content key, and decrypts.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Opake as Opake + FileManager
    participant PDS
    participant Crypto

    User->>CLI: opake download photo.jpg

    CLI->>Opake: ctx.opake() + file_context(None) + file_manager(&ctx)
    Opake->>Opake: mgr.download_at("photo.jpg") — resolve name/path/URI

    Opake->>PDS: com.atproto.repo.getRecord (document)
    PDS-->>Opake: Document record (envelope, blob ref)

    Opake->>Opake: Find wrappedKey matching own DID
    Opake->>Crypto: unwrap_key(wrappedKey, private_key)
    Crypto-->>Opake: content key K

    Opake->>PDS: com.atproto.sync.getBlob (did, cid)
    PDS-->>Opake: ciphertext bytes

    Opake->>Crypto: decrypt_blob(K, nonce, ciphertext)
    Crypto-->>Opake: plaintext

    Note over Opake: #[signoff] auto-persists session if refreshed

    CLI->>CLI: Write plaintext to disk
    CLI->>User: Saved to ./photo.jpg
```

## Download (Shared Files — Cross-PDS)

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

## List

Directory-aware listing with optional workspace, path, tag filter, and long format.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Opake as Opake + FileManager
    participant PDS

    User->>CLI: opake ls Photos --tag vacation --long --workspace family

    CLI->>Opake: ctx.opake() + file_context(Some("family")) + file_manager(&ctx)
    Opake->>Opake: mgr.load_tree() — fetch directory tree
    Opake->>Opake: Resolve "Photos" path in tree

    Opake->>Opake: mgr.resolve_document_names_in(&tree, dir_uri)
    Note over Opake: Lazy per-directory metadata decryption

    Opake->>PDS: getRecord per document child (decrypt name)
    PDS-->>Opake: Document records

    CLI->>CLI: Filter by tag, format output
    CLI->>User: Display table (name, size, tags, URI)
```

## Delete

Deletes a document record. The blob becomes orphaned and is eventually garbage-collected by the PDS. If the document is tracked in a directory, the parent's entry list is updated.

For recursive directory deletion, see [directories.md](directories.md).

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Opake as Opake + FileManager
    participant PDS

    User->>CLI: opake rm photo.jpg

    CLI->>Opake: ctx.opake() + file_context(None) + file_manager(&ctx)
    Opake->>Opake: mgr.load_tree() + mgr.resolve_entry(&tree, "photo.jpg")
    Note over Opake: Lazy resolution — documents resolved one at a time with early exit

    CLI->>User: delete photo.jpg? [y/N]
    User-->>CLI: y

    Opake->>PDS: applyWrites (deleteRecord document + update parent entries)
    PDS-->>Opake: 200 OK

    Note over Opake: #[signoff] auto-persists session if refreshed

    CLI->>User: deleted at://did/.../document/<rkey>
```

For recursive deletion (`rm -r`), `delete_recursive` walks the directory tree in post-order (children before parents), deleting all descendants before the target directory itself. See [directories.md](directories.md#delete-recursive) for details.
