# Example Records

## 0. Public key record (published on login)

Every Opake user publishes their hybrid encryption public key as a singleton record — both halves of the X25519 + ML-KEM-768 KEM. This is how other users discover your key when sharing files with you.

```json
{
  "$type": "at.opake.publicKey",
  "opakeVersion": 1,
  "x25519PublicKey": { "$bytes": "base64-encoded-32-byte-x25519-public-key" },
  "x25519Algo": "x25519",
  "mlKemPublicKey": { "$bytes": "base64-encoded-1184-byte-ml-kem-768-public-key" },
  "mlKemAlgo": "ml-kem-768",
  "createdAt": "2026-03-01T10:00:00.000Z"
}
```

This record uses rkey `self` (like `app.bsky.actor.profile`) — there's only one per account. All fields are required: a record missing either half fails lexicon validation. The keys are published automatically on `opake login`.

## 1. Root directory (created on first `opake mkdir`)

The root directory is a singleton at rkey `self`. Directory names are always encrypted in `encryptedMetadata` — the PDS never sees real folder names. `keyWrapping` tells clients how to unwrap the content key.

```json
{
  "$type": "at.opake.directory",
  "opakeVersion": 1,
  "keyWrapping": {
    "$type": "at.opake.defs#directKeyWrapping",
    "keys": [{
      "did": "did:plc:alice123",
      "ciphertext": { "$bytes": "kv7N...1160 bytes...Q==" },
      "algo": "x25519-mlkem768-hkdf-a256kw-v2"
    }]
  },
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "Ghb8...encrypted JSON..." },
    "nonce": { "$bytes": "rNpK...12 bytes..." }
  },
  "entries": [
    "at://did:plc:alice123/at.opake.directory/3k..."
  ],
  "createdAt": "2026-03-01T10:00:00.000Z",
  "modifiedAt": "2026-03-01T10:00:00.000Z"
}
```

The `entries` array is an ordered list of AT-URIs pointing to documents or other directories (children-on-parent model). Directories use `KeyWrapping` (not `Encryption`) because they have no blob — only `encryptedMetadata`.

## 2. A named directory

```json
{
  "$type": "at.opake.directory",
  "opakeVersion": 1,
  "keyWrapping": {
    "$type": "at.opake.defs#directKeyWrapping",
    "keys": [{
      "did": "did:plc:alice123",
      "ciphertext": { "$bytes": "Xm4p...1160 bytes...==" },
      "algo": "x25519-mlkem768-hkdf-a256kw-v2"
    }]
  },
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "9fHa...encrypted {name:'Photos'}..." },
    "nonce": { "$bytes": "T7kR...12 bytes..." }
  },
  "entries": [
    "at://did:plc:alice123/at.opake.document/3kabcd",
    "at://did:plc:alice123/at.opake.document/3kefgh",
    "at://did:plc:alice123/at.opake.directory/3kijkl"
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
  "$type": "at.opake.directory",
  "opakeVersion": 1,
  "keyWrapping": {
    "$type": "at.opake.defs#keyringKeyWrapping",
    "keyringRef": {
      "keyring": "at://did:plc:alice123/at.opake.keyring/3kabc",
      "wrappedContentKey": { "$bytes": "Qw9f...40 bytes (AES-KW)..." },
      "rotation": 0
    }
  },
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "mNp3...encrypted {name:'Projects'}..." },
    "nonce": { "$bytes": "Hk7a...12 bytes..." }
  },
  "entries": [
    "at://did:plc:alice123/at.opake.document/3kdoc1",
    "at://did:plc:bob456/at.opake.document/3kdoc2"
  ],
  "createdAt": "2026-03-21T10:00:00.000Z"
}
```

Note: `entries` can contain cross-PDS AT-URIs — workspace documents live on each member's PDS. The workspace root directory uses a deterministic rkey `ws-{keyring_rkey}`.

## 2b. Directory update (member proposing structural change)

Non-owner workspace members can't directly modify the owner's directory records. Instead they write `directoryUpdate` proposals to their own PDS. The owner's daemon picks them up via the Indexer and applies them.

```json
{
  "$type": "at.opake.directoryUpdate",
  "opakeVersion": 1,
  "keyring": "at://did:plc:alice123/at.opake.keyring/3kabc",
  "actionType": "addEntry",
  "directory": "at://did:plc:alice123/at.opake.directory/ws-3kabc",
  "entry": "at://did:plc:bob456/at.opake.document/3knewdoc",
  "createdAt": "2026-03-21T12:00:00.000Z"
}
```

Action types: `addEntry`, `removeEntry`, `moveEntry` (with `sourceDirectory` + `targetDirectory`), `createDirectory` (with `parentDirectory` + `encryptedMetadata`), `deleteDirectory`, `renameDirectory` (with `encryptedMetadata`).

## 3. Alice creates a private encrypted document

```json
{
  "$type": "at.opake.document",
  "opakeVersion": 1,
  "blob": {
    "$type": "blob",
    "ref": { "$link": "bafkrei..." },
    "mimeType": "application/octet-stream",
    "size": 284640
  },
  "encryption": {
    "$type": "at.opake.document#directEncryption",
    "envelope": {
      "algo": "aes-256-gcm",
      "nonce": { "$bytes": "base64-encoded-12-byte-nonce" },
      "keys": [
        {
          "did": "did:plc:alice123",
          "ciphertext": { "$bytes": "base64-wrapped-content-key-for-alice" },
          "algo": "x25519-mlkem768-hkdf-a256kw-v2"
        }
      ]
    }
  },
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "base64-aes-256-gcm-encrypted-metadata-json" },
    "nonce": { "$bytes": "base64-encoded-12-byte-nonce" }
  },
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
  "$type": "at.opake.grant",
  "opakeVersion": 1,
  "document": "at://did:plc:alice123/at.opake.document/3k...",
  "recipient": "did:plc:bob456",
  "wrappedKey": {
    "did": "did:plc:bob456",
    "ciphertext": { "$bytes": "base64-wrapped-content-key-for-bob" },
    "algo": "x25519-mlkem768-hkdf-a256kw-v2"
  },
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "base64-aes-256-gcm-encrypted-grant-metadata" },
    "nonce": { "$bytes": "base64-encoded-12-byte-nonce" }
  },
  "createdAt": "2026-02-27T11:00:00.000Z"
}
```

The grant metadata decrypts to the permission and optional note. When the owner
explicitly approves an unverified recipient bundle, it also carries that
bundle's 32-byte approval commitment; it never approves a replacement bundle.

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

Each member is an explicit DID-and-role relationship: manager (full control), editor (upload/edit), or viewer (read-only). A current `wrappedKey` is optional; an admitted member without one can retain historical access but cannot read or create current-generation content until a manager repairs the wrap. An unverified recipient's 32-byte `unverifiedKeyApproval` is optional and binds approval to that exact encryption bundle. The keyring name and description are inside `encryptedMetadata`, encrypted with the group key. This is the pre-v1 record shape; there is no legacy member reader or inferred DID.

```json
{
  "$type": "at.opake.keyring",
  "opakeVersion": 1,
  "algo": "aes-256-gcm",
  "members": [
    {
      "did": "did:plc:alice123",
      "wrappedKey": {
        "did": "did:plc:alice123",
        "ciphertext": { "$bytes": "base64-group-key-wrapped-for-alice" },
        "algo": "x25519-mlkem768-hkdf-a256kw-v2"
      },
      "role": "manager"
    },
    {
      "did": "did:plc:bob456",
      "wrappedKey": {
        "did": "did:plc:bob456",
        "ciphertext": { "$bytes": "base64-group-key-wrapped-for-bob" },
        "algo": "x25519-mlkem768-hkdf-a256kw-v2"
      },
      "unverifiedKeyApproval": { "$bytes": "base64-encoded-32-byte-approval-commitment" },
      "role": "editor"
    },
    {
      "did": "did:plc:carol789",
      "wrappedKey": {
        "did": "did:plc:carol789",
        "ciphertext": { "$bytes": "base64-group-key-wrapped-for-carol" },
        "algo": "x25519-mlkem768-hkdf-a256kw-v2"
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
  "$type": "at.opake.document",
  "opakeVersion": 1,
  "blob": {
    "$type": "blob",
    "ref": { "$link": "bafkrei..." },
    "mimeType": "application/octet-stream",
    "size": 3841056
  },
  "encryption": {
    "$type": "at.opake.document#keyringEncryption",
    "keyringRef": {
      "keyring": "at://did:plc:alice123/at.opake.keyring/3k...",
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


## 5a. Keyring supersede (rotation after removing a member)

A keyring is the head of a supersede chain. The **genesis** keyring's rkey is not a TID — it is a *derived tag* computed from the genesis (rotation-0) group key and the owner DID, so the genesis URI (which is the workspace identity) commits to key material only members hold. Here the genesis lives at:

```
at://did:plc:alice123/at.opake.keyring/452upqgt6ql7ci462dvsfcv6bm
```

That 26-character lowercase-base32 rkey is `base32-lower(SHA-256(Ed25519_pubkey(HKDF(K₀, transcript("opake-workspace-identity", "did:plc:alice123"))))[..16])` — see [CRYPTO.md](../docs/CRYPTO.md#workspace-identity). Any adopting client re-derives it from the record's rotation-0 key and rejects a mismatch.

When manager Bob removes Carol and rotates the group key, he writes a **supersede** on his own PDS. It uses an ordinary TID rkey; `supersedes` points at the prior canonical keyring, `supersedesCid` pins that predecessor's CID, and `lineage` carries the genesis URI unchanged (the workspace identity never moves across a supersede).

```json
{
  "$type": "at.opake.keyring",
  "opakeVersion": 1,
  "algo": "aes-256-gcm",
  "members": [
    {
      "did": "did:plc:alice123",
      "wrappedKey": {
        "did": "did:plc:alice123",
        "ciphertext": { "$bytes": "base64-rotation-1-group-key-for-alice" },
        "algo": "x25519-mlkem768-hkdf-a256kw-v2"
      },
      "role": "manager"
    },
    {
      "did": "did:plc:bob456",
      "wrappedKey": {
        "did": "did:plc:bob456",
        "ciphertext": { "$bytes": "base64-rotation-1-group-key-for-bob" },
        "algo": "x25519-mlkem768-hkdf-a256kw-v2"
      },
      "role": "manager"
    }
  ],
  "rotation": 1,
  "keyHistory": [
    {
      "rotation": 0,
      "members": [
        {
          "did": "did:plc:alice123",
          "wrappedKey": {
            "did": "did:plc:alice123",
            "ciphertext": { "$bytes": "base64-rotation-0-group-key-for-alice" },
            "algo": "x25519-mlkem768-hkdf-a256kw-v2"
          },
          "role": "manager"
        },
        {
          "did": "did:plc:bob456",
          "wrappedKey": {
            "did": "did:plc:bob456",
            "ciphertext": { "$bytes": "base64-rotation-0-group-key-for-bob" },
            "algo": "x25519-mlkem768-hkdf-a256kw-v2"
          },
          "role": "manager"
        }
      ]
    }
  ],
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "base64-aes-256-gcm-encrypted-metadata-json" },
    "nonce": { "$bytes": "base64-encoded-12-byte-nonce" }
  },
  "supersedes": "at://did:plc:alice123/at.opake.keyring/452upqgt6ql7ci462dvsfcv6bm",
  "supersedesCid": "bafyreib2xyz...cid-of-the-genesis-keyring",
  "lineage": "at://did:plc:alice123/at.opake.keyring/452upqgt6ql7ci462dvsfcv6bm",
  "createdAt": "2026-03-22T09:00:00.000Z"
}
```

**Notes:**
- `keyHistory` retains rotation 0's members (minus removed Carol) so surviving members can still decrypt documents uploaded before the rotation. Carol's wrapped key is excluded — she cannot recover the old group key from this record.
- `supersedesCid` is required whenever `supersedes` is present. At v1 it is compared against the CID a serving host *reports* for the fetched predecessor (not a hash recomputed from bytes), so it catches CID disagreement between honest hosts but is not, yet, a defense against a host serving tampered bytes under the true CID — that is deferred to replication-tier work.
- The same `supersedes` + `supersedesCid` pair rides every directory and document supersede, with identical v1 semantics; the pin always names the immediate predecessor and is never copied through a cascade.

## 6. Pair request (new device requesting identity)

A new device generates an ephemeral hybrid keypair (X25519 + ML-KEM-768) and publishes both public halves. The X25519 fingerprint is displayed for visual comparison on both devices.

```json
{
  "$type": "at.opake.pairRequest",
  "opakeVersion": 1,
  "x25519EphemeralKey": { "$bytes": "base64-encoded-32-byte-x25519-ephemeral-public-key" },
  "mlKemEphemeralKey": { "$bytes": "base64-encoded-1184-byte-ml-kem-768-ephemeral-public-key" },
  "algo": "x25519-mlkem768",
  "createdAt": "2026-03-06T14:00:00.000Z"
}
```

This record uses a TID rkey (multiple pending requests are possible). The existing device lists these to show pending requests. Both devices display the X25519 key fingerprint for out-of-band verification — short enough to read aloud or compare on screen, long enough to make collision search infeasible.

## 7. Pair response (existing device sending identity)

The existing device encrypts the full identity (X25519 + ML-KEM-768 + Ed25519 keypairs) and wraps the content key to the ephemeral hybrid bundle from the request, using the same hybrid construction as everywhere else.

```json
{
  "$type": "at.opake.pairResponse",
  "opakeVersion": 1,
  "request": "at://did:plc:alice123/at.opake.pairRequest/3kabcd",
  "wrappedKey": {
    "did": "did:plc:alice123",
    "ciphertext": { "$bytes": "base64-1160-byte-hybrid-wrap-envelope" },
    "algo": "x25519-mlkem768-hkdf-a256kw-v2"
  },
  "ciphertext": { "$bytes": "base64-aes-256-gcm-encrypted-identity-json" },
  "nonce": { "$bytes": "base64-encoded-12-byte-nonce" },
  "algo": "aes-256-gcm",
  "createdAt": "2026-03-06T14:01:00.000Z"
}
```

**How the new device decrypts:**
1. Loads the ephemeral private bundle from local `Storage` (32 + 2400 bytes concatenated, keyed by DID + request rkey)
2. Unwraps the content key via the hybrid construction (X25519 ECDH + ML-KEM-768 Decaps + HKDF combiner)
3. Decrypts the ciphertext with the content key + nonce → identity JSON
4. Verifies the embedded X25519 public key matches the sender's published `publicKey/self` record
5. Saves the identity to disk and wipes the pair state entry

Both PDS records are deleted after successful transfer. The ephemeral private bundle is persisted in `Storage` only between `create_pair_request` and `try_complete_pair` — it has to survive a CLI restart or browser reload while the user walks to the other device, so in-memory alone isn't sufficient. It never crosses the WASM/JS boundary.


## 8. Pending share (recipient hasn't set up Opake yet)

When sharing with someone who has a DID but has not published an Opake key, a `pendingShare` record is created instead of a grant. The owner explicitly permits one first publication at queue time; the daemon retries periodically until that bound recipient publishes an eligible key.

```json
{
  "$type": "at.opake.pendingShare",
  "opakeVersion": 1,
  "document": "at://did:plc:alice123/at.opake.document/3mhborqwpxn22",
  "recipient": "bob.bsky.social",
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "base64-aes-256-gcm-encrypted-grant-metadata" },
    "nonce": { "$bytes": "base64-encoded-12-byte-nonce" }
  },
  "createdAt": "2026-03-20T12:00:00.000Z"
}
```

**Key points:**
- `recipient` stores the handle or DID as the user entered it and is display input only
- `encryptedMetadata` contains `{ permissions: "read", note: "...", recipientDid: "did:plc:bob456", allowUnverifiedFirstPublication: true }` encrypted with the document's content key
- The bound `recipientDid` and explicit first-publication permission prevent a later handle reassignment or a background default from redirecting the handoff
- No `wrappedKey` — the content key can't be wrapped until the recipient publishes their public key
- The daemon re-derives the content key from the document at retry time using the owner's identity
- Completion conditionally creates one designated grant and deletes the unchanged intent together; cancellation, replacement, expiry, and retry cannot create another grant
- Records expire after 7 days. A conditional expiry deletes the still-current intent; verification failures are reported to the owner instead of being silently treated as ordinary retry failures
- Cross-device: created from any device, retried by any device with the daemon running

## 9. Document update (collaborative editing)

An editor proposes an update to a document owned by another workspace member. The update lives on the editor's PDS until the owner applies it.

```json
{
  "$type": "at.opake.documentUpdate",
  "opakeVersion": 1,
  "document": "at://did:plc:alice123/at.opake.document/3kabcd",
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
  "$type": "at.opake.documentUpdate",
  "opakeVersion": 1,
  "document": "at://did:plc:alice123/at.opake.document/3kabcd",
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
  "supersedes": "at://did:plc:removed-member/at.opake.document/3koriginal",
  "createdAt": "2026-03-21T10:30:00.000Z"
}
```

## 10. Leaving a workspace

A member opts out of a workspace by writing a `keyringUpdate` record with action `leave` to their own PDS. The indexer removes them from the workspace's member list.

```json
{
  "$type": "at.opake.keyringUpdate",
  "opakeVersion": 1,
  "keyring": "at://did:plc:alice123/at.opake.keyring/3k...",
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
