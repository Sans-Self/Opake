# Document Operations

## Upload

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

## Download (Own Files)

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

## Delete

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
