# Example Records

## 0. Public key record (published on login)

Every Opake user publishes their X25519 encryption public key as a singleton record. This is how other users discover your key when sharing files with you.

```json
{
  "$type": "app.opake.publicKey",
  "opakeVersion": 1,
  "publicKey": { "$bytes": "base64-encoded-32-byte-x25519-public-key" },
  "algo": "x25519",
  "createdAt": "2026-03-01T10:00:00.000Z"
}
```

This record uses rkey `self` (like `app.bsky.actor.profile`) — there's only one per account. The key is published automatically on `opake login`.

## 1. Root directory (created on first `opake mkdir`)

The root directory is a singleton at rkey `self`. Directory names are always encrypted in `encryptedMetadata` — the PDS never sees real folder names. `keyWrapping` tells clients how to unwrap the content key.

```json
{
  "$type": "app.opake.directory",
  "opakeVersion": 1,
  "keyWrapping": {
    "$type": "app.opake.defs#directKeyWrapping",
    "keys": [{
      "did": "did:plc:alice123",
      "ciphertext": { "$bytes": "kv7N...72 bytes...Q==" },
      "algo": "x25519-hkdf-a256kw"
    }]
  },
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "Ghb8...encrypted JSON..." },
    "nonce": { "$bytes": "rNpK...12 bytes..." }
  },
  "entries": [
    "at://did:plc:alice123/app.opake.directory/3k..."
  ],
  "createdAt": "2026-03-01T10:00:00.000Z",
  "modifiedAt": "2026-03-01T10:00:00.000Z"
}
```

The `entries` array is an ordered list of AT-URIs pointing to documents or other directories (children-on-parent model). Directories use `KeyWrapping` (not `Encryption`) because they have no blob — only `encryptedMetadata`.

## 2. A named directory

```json
{
  "$type": "app.opake.directory",
  "opakeVersion": 1,
  "keyWrapping": {
    "$type": "app.opake.defs#directKeyWrapping",
    "keys": [{
      "did": "did:plc:alice123",
      "ciphertext": { "$bytes": "Xm4p...72 bytes...==" },
      "algo": "x25519-hkdf-a256kw"
    }]
  },
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "9fHa...encrypted {name:'Photos'}..." },
    "nonce": { "$bytes": "T7kR...12 bytes..." }
  },
  "entries": [
    "at://did:plc:alice123/app.opake.document/3kabcd",
    "at://did:plc:alice123/app.opake.document/3kefgh",
    "at://did:plc:alice123/app.opake.directory/3kijkl"
  ],
  "createdAt": "2026-03-01T10:05:00.000Z",
  "modifiedAt": "2026-03-01T11:30:00.000Z"
}
```

This directory contains two documents and a subdirectory. Non-root directories use TID rkeys (created via `createRecord`). The decrypted `encryptedMetadata` contains `{ "name": "Photos" }`.

## 2a. Workspace directory (encrypted under keyring)

A workspace directory uses `keyringKeyWrapping` — the content key is wrapped under the workspace's group key instead of an individual DID's public key. All workspace members who can unwrap the group key can read the directory name.

```json
{
  "$type": "app.opake.directory",
  "opakeVersion": 1,
  "keyWrapping": {
    "$type": "app.opake.defs#keyringKeyWrapping",
    "keyringRef": {
      "keyring": "at://did:plc:alice123/app.opake.keyring/3kabc",
      "wrappedContentKey": { "$bytes": "Qw9f...40 bytes (AES-KW)..." },
      "rotation": 0
    }
  },
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "mNp3...encrypted {name:'Projects'}..." },
    "nonce": { "$bytes": "Hk7a...12 bytes..." }
  },
  "entries": [
    "at://did:plc:alice123/app.opake.document/3kdoc1",
    "at://did:plc:bob456/app.opake.document/3kdoc2"
  ],
  "createdAt": "2026-03-21T10:00:00.000Z"
}
```

Note: `entries` can contain cross-PDS AT-URIs — workspace documents live on each member's PDS. The workspace root directory uses a deterministic rkey `ws-{keyring_rkey}`.

## 2b. Directory update (member proposing structural change)

Non-owner workspace members can't directly modify the owner's directory records. Instead they write `directoryUpdate` proposals to their own PDS. The owner's daemon picks them up via the Indexer and applies them.

```json
{
  "$type": "app.opake.directoryUpdate",
  "opakeVersion": 1,
  "keyring": "at://did:plc:alice123/app.opake.keyring/3kabc",
  "actionType": "addEntry",
  "directory": "at://did:plc:alice123/app.opake.directory/ws-3kabc",
  "entry": "at://did:plc:bob456/app.opake.document/3knewdoc",
  "createdAt": "2026-03-21T12:00:00.000Z"
}
```

Action types: `addEntry`, `removeEntry`, `moveEntry` (with `sourceDirectory` + `targetDirectory`), `createDirectory` (with `parentDirectory` + `encryptedMetadata`), `deleteDirectory`, `renameDirectory` (with `encryptedMetadata`).

## 3. Alice creates a private encrypted document

```json
{
  "$type": "app.opake.document",
  "opakeVersion": 1,
  "blob": {
    "$type": "blob",
    "ref": { "$link": "bafkrei..." },
    "mimeType": "application/octet-stream",
    "size": 284640
  },
  "encryption": {
    "$type": "app.opake.document#directEncryption",
    "envelope": {
      "algo": "aes-256-gcm",
      "nonce": { "$bytes": "base64-encoded-12-byte-nonce" },
      "keys": [
        {
          "did": "did:plc:alice123",
          "ciphertext": { "$bytes": "base64-wrapped-content-key-for-alice" },
          "algo": "x25519-hkdf-a256kw"
        }
      ]
    }
  },
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "base64-aes-256-gcm-encrypted-metadata-json" },
    "nonce": { "$bytes": "base64-encoded-12-byte-nonce" }
  },
  "visibility": "private",
  "createdAt": "2026-02-27T10:30:00.000Z"
}
```

**What the PDS sees:** a record with an opaque blob and opaque encrypted metadata.
The real filename ("tax-return-2025.pdf"), MIME type, size, and tags are all inside
`encryptedMetadata`, encrypted with the same content key as the blob. The `keys`
array only contains Alice's wrapped key — only she can decrypt.


## 4. Alice shares the document with Bob via a grant

```json
{
  "$type": "app.opake.grant",
  "opakeVersion": 1,
  "document": "at://did:plc:alice123/app.opake.document/3k...",
  "recipient": "did:plc:bob456",
  "wrappedKey": {
    "did": "did:plc:bob456",
    "ciphertext": { "$bytes": "base64-wrapped-content-key-for-bob" },
    "algo": "x25519-hkdf-a256kw"
  },
  "permissions": "read",
  "note": "Here's the tax doc you asked about",
  "createdAt": "2026-02-27T11:00:00.000Z"
}
```

**How Bob decrypts:**
1. His client/Indexer discovers this grant (firehose, query, or notification)
2. Fetches the document record via the `document` AT URI
3. Uses his private key to decrypt `wrappedKey.ciphertext` → gets AES-256 content key
4. Fetches the blob via `com.atproto.sync.getBlob`
5. Decrypts the blob using the content key + nonce from the document's encryption envelope

**To revoke:** Alice deletes the grant record. Bob's copy of the wrapped key is gone
from the network (eventually). For true forward secrecy, Alice would also re-encrypt
the document with a fresh content key.


## 5. Workspace (keyring-based group sharing)

### The keyring record:

The `owner` field identifies the canonical owner (Alice). Each member has a `role`: manager (full control), editor (upload/edit), or viewer (read-only). The keyring name and description are inside `encryptedMetadata`, encrypted with the group key.

```json
{
  "$type": "app.opake.keyring",
  "opakeVersion": 1,
  "algo": "aes-256-gcm",
  "owner": "did:plc:alice123",
  "members": [
    {
      "wrappedKey": {
        "did": "did:plc:alice123",
        "ciphertext": { "$bytes": "base64-group-key-wrapped-for-alice" },
        "algo": "x25519-hkdf-a256kw"
      },
      "role": "manager"
    },
    {
      "wrappedKey": {
        "did": "did:plc:bob456",
        "ciphertext": { "$bytes": "base64-group-key-wrapped-for-bob" },
        "algo": "x25519-hkdf-a256kw"
      },
      "role": "editor"
    },
    {
      "wrappedKey": {
        "did": "did:plc:carol789",
        "ciphertext": { "$bytes": "base64-group-key-wrapped-for-carol" },
        "algo": "x25519-hkdf-a256kw"
      },
      "role": "viewer"
    }
  ],
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "base64-aes-256-gcm-encrypted-metadata-json" },
    "nonce": { "$bytes": "base64-encoded-12-byte-nonce" }
  },
  "rotation": 0,
  "createdAt": "2026-01-15T09:00:00.000Z"
}
```

### Then, a document using the keyring:

```json
{
  "$type": "app.opake.document",
  "opakeVersion": 1,
  "blob": {
    "$type": "blob",
    "ref": { "$link": "bafkrei..." },
    "mimeType": "application/octet-stream",
    "size": 3841056
  },
  "encryption": {
    "$type": "app.opake.document#keyringEncryption",
    "keyringRef": {
      "keyring": "at://did:plc:alice123/app.opake.keyring/3k...",
      "wrappedContentKey": { "$bytes": "base64-content-key-encrypted-with-group-key" },
      "rotation": 0
    },
    "algo": "aes-256-gcm",
    "nonce": { "$bytes": "base64-encoded-12-byte-nonce" }
  },
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "base64-aes-256-gcm-encrypted-metadata-json" },
    "nonce": { "$bytes": "base64-encoded-12-byte-nonce" }
  },
  "visibility": "shared",
  "createdAt": "2026-02-20T16:45:00.000Z"
}
```

**How any family member decrypts:**
1. Fetch the keyring record from the `keyring` AT URI
2. Find their own entry in `members`, decrypt with their private key → get group key GK
3. Decrypt `wrappedContentKey` with GK → get the per-document content key
4. Fetch + decrypt the blob with content key + nonce

**Adding a new member (did:plc:dave as editor):**
- Wrap GK to Dave's pubkey with `"role": "editor"`
- Update the keyring record to add Dave to `members`
- Dave can now decrypt *all* documents under this keyring. No per-document changes needed.
- The Indexer enforces Dave's role — he can propose edits via `documentUpdate` but can't add/remove members.

**Removing a member:**
- Archive the current rotation's remaining member entries into `keyHistory`
- Increment `rotation`, generate new GK, re-wrap to remaining members
- New documents use the new GK
- Old documents remain readable: the client looks up the document's rotation in `keyHistory` to find the old wrapped group key
- Removed members' wrapped keys are excluded from history, so they can't recover old GK from the record
- Forward secrecy is automatic (removed member can't decrypt new content). For historical access revocation, see background re-encryption.


## 6. Pair request (new device requesting identity)

A new device generates an ephemeral X25519 keypair and publishes the public half. The fingerprint is displayed for visual comparison on both devices.

```json
{
  "$type": "app.opake.pairRequest",
  "opakeVersion": 1,
  "ephemeralKey": { "$bytes": "base64-encoded-32-byte-x25519-ephemeral-public-key" },
  "algo": "x25519",
  "createdAt": "2026-03-06T14:00:00.000Z"
}
```

This record uses a TID rkey (multiple pending requests are possible). The existing device lists these to show pending requests. Both devices display the key fingerprint for out-of-band verification.

## 7. Pair response (existing device sending identity)

The existing device encrypts the full identity (X25519 + Ed25519 keypairs) and wraps the content key to the ephemeral public key from the request.

```json
{
  "$type": "app.opake.pairResponse",
  "opakeVersion": 1,
  "request": "at://did:plc:alice123/app.opake.pairRequest/3kabcd",
  "wrappedKey": {
    "did": "did:plc:alice123",
    "ciphertext": { "$bytes": "base64-content-key-wrapped-to-ephemeral-pubkey" },
    "algo": "x25519-hkdf-a256kw"
  },
  "ciphertext": { "$bytes": "base64-aes-256-gcm-encrypted-identity-json" },
  "nonce": { "$bytes": "base64-encoded-12-byte-nonce" },
  "algo": "aes-256-gcm",
  "createdAt": "2026-03-06T14:01:00.000Z"
}
```

**How the new device decrypts:**
1. Loads the ephemeral private key from local `Storage` (keyed by DID + request rkey)
2. Unwraps the content key using that private key
3. Decrypts the ciphertext with the content key + nonce → identity JSON
4. Verifies the derived public key matches the published `publicKey/self` record
5. Saves the identity to disk and wipes the pair state entry

Both PDS records are deleted after successful transfer. The ephemeral private key is persisted in `Storage` only between `create_pair_request` and `try_complete_pair` — it has to survive a CLI restart or browser reload while the user walks to the other device, so in-memory alone isn't sufficient. It never crosses the WASM/JS boundary.


## 8. Pending share (recipient hasn't set up Opake yet)

When sharing with someone who hasn't logged into Opake, a `pendingShare` record is created instead of a grant. The daemon retries periodically until the recipient publishes their public key.

```json
{
  "$type": "app.opake.pendingShare",
  "opakeVersion": 1,
  "document": "at://did:plc:alice123/app.opake.document/3mhborqwpxn22",
  "recipient": "bob.bsky.social",
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "base64-aes-256-gcm-encrypted-grant-metadata" },
    "nonce": { "$bytes": "base64-encoded-12-byte-nonce" }
  },
  "createdAt": "2026-03-20T12:00:00.000Z"
}
```

**Key points:**
- `recipient` stores the handle or DID as the user entered it (not necessarily a DID)
- `encryptedMetadata` contains `{ permissions: "read", note: "..." }` encrypted with the document's content key — same format as a grant's metadata
- No `wrappedKey` — the content key can't be wrapped until the recipient publishes their public key
- The daemon re-derives the content key from the document at retry time using the owner's identity
- Records expire after 7 days and are automatically deleted by the daemon
- Cross-device: created from any device, retried by any device with the daemon running

## 9. Document update (collaborative editing)

An editor proposes an update to a document owned by another workspace member. The update lives on the editor's PDS until the owner applies it.

```json
{
  "$type": "app.opake.documentUpdate",
  "opakeVersion": 1,
  "document": "at://did:plc:alice123/app.opake.document/3kabcd",
  "blob": {
    "$type": "blob",
    "ref": { "$link": "bafkrei..." },
    "mimeType": "application/octet-stream",
    "size": 294912
  },
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "base64-aes-256-gcm-encrypted-metadata-json" },
    "nonce": { "$bytes": "base64-encoded-12-byte-nonce" }
  },
  "createdAt": "2026-03-21T10:00:00.000Z"
}
```

**How the owner applies it:**
1. Indexer surfaces pending updates via `GET /api/workspace/updates`
2. Owner's client fetches the update blob from the editor's PDS
3. Owner re-uploads the blob to their own PDS and updates their document record
4. Editor's client deletes the `documentUpdate` record after confirmation

For document adoption (when a member is removed), the `supersedes` field points to the original document URI being replaced:

```json
{
  "$type": "app.opake.documentUpdate",
  "opakeVersion": 1,
  "document": "at://did:plc:alice123/app.opake.document/3kabcd",
  "blob": {
    "$type": "blob",
    "ref": { "$link": "bafkrei..." },
    "mimeType": "application/octet-stream",
    "size": 294912
  },
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "base64-aes-256-gcm-encrypted-metadata-json" },
    "nonce": { "$bytes": "base64-encoded-12-byte-nonce" }
  },
  "supersedes": "at://did:plc:removed-member/app.opake.document/3koriginal",
  "createdAt": "2026-03-21T10:30:00.000Z"
}
```

## 10. Leaving a workspace

A member opts out of a workspace by writing a `keyringUpdate` record with action `leave` to their own PDS. The indexer removes them from the workspace's member list.

```json
{
  "$type": "app.opake.keyringUpdate",
  "opakeVersion": 1,
  "keyring": "at://did:plc:alice123/app.opake.keyring/3k...",
  "actionType": "leave",
  "createdAt": "2026-03-21T11:00:00.000Z"
}
```

**Key points:**
- `leave` is one of the action types on the unified `keyringUpdate` record (alongside `addMember`, `removeMember`, `updateRole`, `rename`, `updateDescription`).
- This is a visibility opt-out, not a key revocation — the member's wrapped key still exists on the keyring record until the owner processes the proposal and rotates the group key.
- The workspace disappears from the member's sidebar once the indexer processes the record.
- Used for both voluntary leave and cleaning up stale/forked workspace membership.

## Design Decisions & Notes

### Why encrypted metadata?
All document metadata — name, MIME type, size, tags, description — is encrypted
inside `encryptedMetadata` using the same content key as the blob. The PDS never
sees real filenames or tags. This means server-side search and indexing require
client-side decryption, but no metadata is ever leaked to the storage layer.

### Why separate grant records?
Instead of adding recipients directly to the document record (like adding to the
`keys` array), grants are separate records because:
- The document owner might not want to update the document record every time they share
- Grants can be deleted independently (for revocation)
- An Indexer can efficiently query "what's shared with me?" across all documents
- It matches the atproto pattern of small, independent records

### Why the two-layer key for keyrings?
Documents under a keyring still have their own per-document content key, just
wrapped under the group key instead of individual pubkeys. This means:
- Rotating the group key doesn't require re-encrypting every document's content
- Individual documents can be selectively re-encrypted without touching the keyring
- The content key acts as a per-document nonce for the group key
