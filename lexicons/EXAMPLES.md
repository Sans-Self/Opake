# Example Records

## 1. Alice creates a private encrypted document

```json
{
  "$type": "app.opake.cloud.document",
  "name": "tax-return-2025.pdf",
  "mimeType": "application/pdf",
  "size": 284619,
  "blob": {
    "$type": "blob",
    "ref": { "$link": "bafkrei..." },
    "mimeType": "application/octet-stream",
    "size": 284640
  },
  "encryption": {
    "$type": "app.opake.cloud.document#directEncryption",
    "envelope": {
      "algo": "aes-256-gcm",
      "nonce": { "$bytes": "base64-encoded-12-byte-nonce" },
      "keys": [
        {
          "did": "did:plc:alice123",
          "ciphertext": { "$bytes": "base64-wrapped-content-key-for-alice" },
          "algo": "ECDH-ES+A256KW"
        }
      ]
    }
  },
  "tags": ["tax", "finance", "2025"],
  "visibility": "private",
  "createdAt": "2026-02-27T10:30:00.000Z"
}
```

**What the PDS sees:** a record with some plaintext metadata (name, tags, timestamps)
and an opaque blob. The `keys` array only contains Alice's wrapped key — only she
can decrypt.


## 2. Alice shares the document with Bob via a grant

```json
{
  "$type": "app.opake.cloud.grant",
  "document": "at://did:plc:alice123/app.opake.cloud.document/3k...",
  "recipient": "did:plc:bob456",
  "wrappedKey": {
    "did": "did:plc:bob456",
    "ciphertext": { "$bytes": "base64-wrapped-content-key-for-bob" },
    "algo": "ECDH-ES+A256KW"
  },
  "permissions": "read",
  "note": "Here's the tax doc you asked about",
  "createdAt": "2026-02-27T11:00:00.000Z"
}
```

**How Bob decrypts:**
1. His client/AppView discovers this grant (firehose, query, or notification)
2. Fetches the document record via the `document` AT URI
3. Uses his private key to decrypt `wrappedKey.ciphertext` → gets AES-256 content key
4. Fetches the blob via `com.atproto.sync.getBlob`
5. Decrypts the blob using the content key + nonce from the document's encryption envelope

**To revoke:** Alice deletes the grant record. Bob's copy of the wrapped key is gone
from the network (eventually). For true forward secrecy, Alice would also re-encrypt
the document with a fresh content key.


## 3. Keyring-based group sharing (family photos)

### First, the keyring:

```json
{
  "$type": "app.opake.cloud.keyring",
  "name": "family-photos",
  "description": "Shared photo collection for the family",
  "algo": "aes-256-gcm",
  "members": [
    {
      "did": "did:plc:alice123",
      "ciphertext": { "$bytes": "base64-group-key-wrapped-for-alice" },
      "algo": "ECDH-ES+A256KW"
    },
    {
      "did": "did:plc:bob456",
      "ciphertext": { "$bytes": "base64-group-key-wrapped-for-bob" },
      "algo": "ECDH-ES+A256KW"
    },
    {
      "did": "did:plc:carol789",
      "ciphertext": { "$bytes": "base64-group-key-wrapped-for-carol" },
      "algo": "ECDH-ES+A256KW"
    }
  ],
  "rotation": 0,
  "createdAt": "2026-01-15T09:00:00.000Z"
}
```

### Then, a document using the keyring:

```json
{
  "$type": "app.opake.cloud.document",
  "name": "beach-sunset.jpg",
  "mimeType": "image/jpeg",
  "size": 3841029,
  "blob": {
    "$type": "blob",
    "ref": { "$link": "bafkrei..." },
    "mimeType": "application/octet-stream",
    "size": 3841056
  },
  "encryption": {
    "$type": "app.opake.cloud.document#keyringEncryption",
    "keyringRef": {
      "keyring": "at://did:plc:alice123/app.opake.cloud.keyring/3k...",
      "wrappedContentKey": { "$bytes": "base64-content-key-encrypted-with-group-key" },
      "rotation": 0
    },
    "algo": "aes-256-gcm",
    "nonce": { "$bytes": "base64-encoded-12-byte-nonce" }
  },
  "tags": ["family", "vacation", "beach"],
  "visibility": "shared",
  "createdAt": "2026-02-20T16:45:00.000Z"
}
```

**How any family member decrypts:**
1. Fetch the keyring record from the `keyring` AT URI
2. Find their own entry in `members`, decrypt with their private key → get group key GK
3. Decrypt `wrappedContentKey` with GK → get the per-document content key
4. Fetch + decrypt the blob with content key + nonce

**Adding a new family member (did:plc:dave):**
- Wrap GK to Dave's pubkey
- Update the keyring record to add Dave to `members`
- Dave can now decrypt *all* documents under this keyring. No per-document changes needed.

**Removing a member:**
- Increment `rotation`, generate new GK, re-wrap to remaining members
- New documents use the new GK
- Old documents remain readable with old GK (same limitation as git-crypt)
- For true revocation of old content: re-encrypt affected documents with new content keys


## Design Decisions & Notes

### Why plaintext metadata?
The `name`, `tags`, `mimeType`, and `description` fields are intentionally unencrypted.
This allows your personal AppView to index and search your files server-side without
needing access to the content encryption keys. It's a conscious tradeoff: someone
inspecting your repo can see *that* you have a file called "tax-return-2025.pdf" tagged
with "finance", but they can't read the actual PDF.

If you want fully opaque storage, you can encrypt the name/tags too and handle
search purely client-side. The schema supports this — just put garbage/generic
strings in the plaintext fields and store the real metadata inside the encrypted blob.

### Why separate grant records?
Instead of adding recipients directly to the document record (like adding to the
`keys` array), grants are separate records because:
- The document owner might not want to update the document record every time they share
- Grants can be deleted independently (for revocation)
- An AppView can efficiently query "what's shared with me?" across all documents
- It matches the atproto pattern of small, independent records

### Why the two-layer key for keyrings?
Documents under a keyring still have their own per-document content key, just
wrapped under the group key instead of individual pubkeys. This means:
- Rotating the group key doesn't require re-encrypting every document's content
- Individual documents can be selectively re-encrypted without touching the keyring
- The content key acts as a per-document nonce for the group key
