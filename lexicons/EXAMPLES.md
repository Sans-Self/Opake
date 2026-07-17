# Example Records

Every record below is a real instance of an `at.opake.*` lexicon, annotated with what
the PDS sees versus what only a keyholder can read. The `$bytes` values are placeholders
for base64-encoded binary; lengths are called out where they matter.

## 0. Public key record (published on login)

Every Opake user publishes their hybrid encryption public key as a singleton — both
halves of the X25519 + ML-KEM-768 KEM. This is how another user discovers your key when
sharing a file with you.

```json
{
  "$type": "at.opake.publicKey",
  "opakeVersion": 1,
  "x25519PublicKey": { "$bytes": "base64-encoded-32-byte-x25519-public-key" },
  "x25519Algo": "x25519",
  "mlKemPublicKey": { "$bytes": "base64-encoded-1184-byte-ml-kem-768-public-key" },
  "mlKemAlgo": "ml-kem-768",
  "signingKey": { "$bytes": "base64-encoded-32-byte-ed25519-public-key" },
  "signingAlgo": "ed25519",
  "createdAt": "2026-03-01T10:00:00.000Z"
}
```

The record uses rkey `self` — one per account. The two KEM halves and their algorithm
identifiers are required; a record missing either half fails lexicon validation. The
`signingKey`/`signingAlgo` pair is optional: it carries the Ed25519 public key the
indexer uses to authenticate DID-scoped calls, so a client that never talks to an indexer
can omit it. The keys are published automatically on `opake login`.

## 1. Root directory (created on first `opake mkdir`)

The root directory of a personal cabinet is a singleton at rkey `self`. Directory names
are always encrypted in `encryptedMetadata` — the PDS never sees a real folder name.
`keyWrapping` tells a client how to unwrap the content key that protects that metadata.

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
    {
      "target": "at://did:plc:alice123/at.opake.directory/3kphotos",
      "targetCid": "bafyreib2...cid-of-photos-directory"
    }
  ],
  "createdAt": "2026-03-01T10:00:00.000Z",
  "modifiedAt": "2026-03-01T10:00:00.000Z"
}
```

Each entry pins an AT-URI plus the CID of the target record at the moment this directory
was written. Targets are documents or other directories; for a nested directory the
`targetCid` is the head of that child path's chain at write time. Directories use
`KeyWrapping` (not the full encryption envelope) because they carry no blob — only
`encryptedMetadata` needs a key.

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
    {
      "target": "at://did:plc:alice123/at.opake.document/3kabcd",
      "targetCid": "bafyreic1...cid-of-doc-1"
    },
    {
      "target": "at://did:plc:alice123/at.opake.document/3kefgh",
      "targetCid": "bafyreic2...cid-of-doc-2"
    },
    {
      "target": "at://did:plc:alice123/at.opake.directory/3kijkl",
      "targetCid": "bafyreic3...cid-of-subdirectory-head"
    }
  ],
  "createdAt": "2026-03-01T10:05:00.000Z",
  "modifiedAt": "2026-03-01T11:30:00.000Z"
}
```

This directory holds two documents and a subdirectory. Non-root cabinet directories use
TID rkeys. The decrypted `encryptedMetadata` is `{ "name": "Photos" }`.

## 2a. Workspace directory (encrypted under a keyring)

A workspace directory uses `keyringKeyWrapping`: the content key is wrapped under the
workspace's group key instead of an individual DID's public key, so every member who can
unwrap the group key can read the directory name.

```json
{
  "$type": "at.opake.directory",
  "opakeVersion": 1,
  "keyWrapping": {
    "$type": "at.opake.defs#keyringKeyWrapping",
    "keyringRef": {
      "keyring": "at://did:plc:alice123/at.opake.keyring/6ywb55pjvb2xuzdbsy7udewfrq",
      "wrappedContentKey": { "$bytes": "Qw9f...40 bytes (AES-KW)..." },
      "rotation": 0
    }
  },
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "mNp3...encrypted {name:'Projects'}..." },
    "nonce": { "$bytes": "Hk7a...12 bytes..." }
  },
  "entries": [
    {
      "target": "at://did:plc:alice123/at.opake.document/3kdoc1",
      "targetCid": "bafyreid1...cid-of-doc-1"
    },
    {
      "target": "at://did:plc:bob456/at.opake.document/3kdoc2",
      "targetCid": "bafyreid2...cid-of-doc-2"
    }
  ],
  "workspaceId": "at://did:plc:alice123/at.opake.keyring/6ywb55pjvb2xuzdbsy7udewfrq",
  "isWorkspaceRoot": true,
  "createdAt": "2026-03-21T10:00:00.000Z"
}
```

`entries` can point at cross-PDS AT-URIs — workspace documents live on each member's own
PDS. `workspaceId` names the genesis keyring so any reader resolves the workspace without
walking the keyring chain. `isWorkspaceRoot: true` marks this directory as part of the
workspace-root chain; the indexer enforces that only a manager may set it, that the flag
never flips across a supersede, and that a workspace has at most one active root chain
(`spec:tree-chains § The workspace root is a flag-marked chain, forward-walked from
genesis`). The root is an ordinary TID-rkeyed record, forward-walked from genesis — there
is no reserved or derived root rkey.

## 2b. Workspace directory supersede (reorganizing a shared folder)

A workspace directory at a given path is the head of a supersede chain: the canonical
state is whichever record has no successor pointing at it (`spec:tree-chains § A path's
canonical state is the head of a supersede chain`). A member reorganizing the folder
writes a new directory record on their own PDS that supersedes the current head — no
proposal, no owner round-trip.

```json
{
  "$type": "at.opake.directory",
  "opakeVersion": 1,
  "keyWrapping": {
    "$type": "at.opake.defs#keyringKeyWrapping",
    "keyringRef": {
      "keyring": "at://did:plc:alice123/at.opake.keyring/6ywb55pjvb2xuzdbsy7udewfrq",
      "wrappedContentKey": { "$bytes": "Qw9f...40 bytes (AES-KW)..." },
      "rotation": 0
    }
  },
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "mNp3...encrypted {name:'Projects'}..." },
    "nonce": { "$bytes": "Hk7a...12 bytes..." }
  },
  "entries": [
    {
      "target": "at://did:plc:alice123/at.opake.document/3kdoc1",
      "targetCid": "bafyreid1...cid-of-doc-1"
    },
    {
      "target": "at://did:plc:bob456/at.opake.document/3knewdoc",
      "targetCid": "bafyreid9...cid-of-new-doc"
    }
  ],
  "workspaceId": "at://did:plc:alice123/at.opake.keyring/6ywb55pjvb2xuzdbsy7udewfrq",
  "isWorkspaceRoot": true,
  "supersedes": "at://did:plc:alice123/at.opake.directory/3kprevroot",
  "supersedesCid": "bafyreid0...cid-of-the-superseded-directory",
  "lineage": "at://did:plc:alice123/at.opake.directory/3kgenesisroot",
  "createdAt": "2026-03-21T12:00:00.000Z"
}
```

`supersedes` names the prior head; `supersedesCid` pins that predecessor's CID and is
required whenever `supersedes` is present (`spec:lineage § Supersede references carry a
content pin`); `lineage` carries the chain's genesis URI unchanged — the directory's stable
object identity, which never moves across a supersede (`spec:lineage § Lineage never flips
across a supersede`). The metadata ciphertext is AEAD-bound to that genesis anchor, which
is why a ciphertext copied verbatim across a supersede still authenticates. The indexer
validates authority at write time: an editor's supersede must be additive, a manager's is
unrestricted (`spec:tree-chains § Editor supersedes are additive; managers are
unrestricted`).

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

**What the PDS sees:** a record with an opaque blob and opaque encrypted metadata. The
real filename (`tax-return-2025.pdf`), MIME type, size, and tags all live inside
`encryptedMetadata`, encrypted with the same content key as the blob
(`spec:document-crypto § All document metadata is encrypted`). The `keys` array holds only
Alice's wrapped key — only she can decrypt.

## 4. Alice shares the document with Bob via a grant

A grant is a standalone record, not inline document state — it can be created and revoked
without rewriting the document (`spec:sharing-grants § A grant is a standalone record, not
inline document state`).

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
    "ciphertext": { "$bytes": "base64-encrypted {permissions:'read', note:'...'}..." },
    "nonce": { "$bytes": "base64-encoded-12-byte-nonce" }
  },
  "createdAt": "2026-02-27T11:00:00.000Z"
}
```

The grant's own metadata — the permission level and any note — lives inside
`encryptedMetadata`, encrypted with the document's content key so both grantor and
recipient can read it and the PDS cannot. The record-level fields carry only what the
network legitimately needs to route the share: the document URI and the recipient DID.

**How Bob decrypts:**
1. His indexer surfaces the grant (`spec:sharing-grants § The recipient discovers shares
   through the indexer, not by polling PDSes`).
2. He fetches the document record via the `document` AT-URI.
3. He unwraps `wrappedKey.ciphertext` with his private key → the AES-256 content key.
4. He fetches the blob via `com.atproto.sync.getBlob`.
5. He decrypts the blob with the content key and the nonce from the document's envelope.

**To revoke:** Alice deletes the grant. Bob stops discovering it, but a copy he already
unwrapped is beyond recall — revocation stops future discovery, not historical access
(`spec:sharing-grants § Revocation stops future discovery but not historical access`). For
true forward secrecy Alice re-encrypts the document under a fresh content key.

## 5. Workspace (keyring-based group sharing)

### The keyring record

A keyring holds a group symmetric key wrapped to each member, paired with that member's
role. There is no owner field: authority is the chain itself, and every member carries one
of three roles — manager (full control), editor (upload and edit), or viewer (read-only)
(`spec:workspace-membership § Three roles, no owner`). Role is plaintext because the
indexer needs it for authorization; the keyring name and description are encrypted under
the group key in `encryptedMetadata`.

```json
{
  "$type": "at.opake.keyring",
  "opakeVersion": 1,
  "algo": "aes-256-gcm",
  "members": [
    {
      "wrappedKey": {
        "did": "did:plc:alice123",
        "ciphertext": { "$bytes": "base64-group-key-wrapped-for-alice" },
        "algo": "x25519-mlkem768-hkdf-a256kw-v2"
      },
      "role": "manager"
    },
    {
      "wrappedKey": {
        "did": "did:plc:bob456",
        "ciphertext": { "$bytes": "base64-group-key-wrapped-for-bob" },
        "algo": "x25519-mlkem768-hkdf-a256kw-v2"
      },
      "role": "editor"
    },
    {
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

The genesis keyring's rkey is not a TID — it is a derived tag computed from the genesis
(rotation-0) group key and the founding member's DID, so the genesis URI (which *is* the
workspace identity) commits to key material only members hold (`spec:workspace-identity §
Genesis URI is the workspace identity`). See [§ 5a](#5a-keyring-supersede-rotation-after-removing-a-member)
for the derivation and an example rkey.

### A document under the keyring

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
      "keyring": "at://did:plc:alice123/at.opake.keyring/6ywb55pjvb2xuzdbsy7udewfrq",
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
  "workspaceId": "at://did:plc:alice123/at.opake.keyring/6ywb55pjvb2xuzdbsy7udewfrq",
  "createdAt": "2026-02-20T16:45:00.000Z"
}
```

**How any member decrypts:**
1. Fetch the keyring record from the `keyring` AT-URI.
2. Find their own entry in `members`, unwrap it with their private key → the group key GK.
3. Decrypt `wrappedContentKey` with GK → the per-document content key.
4. Fetch and decrypt the blob with the content key and nonce.

The `rotation` on `keyringRef` selects which generation of the group key to use, so a
reader admitted after several rotations still resolves the right key for an old document
(`spec:document-crypto § Keyring reads select the group key by the document's rotation`).

**Adding a member.** A manager writes a keyring supersede whose `members` array includes
the new member's wrapped group key (`spec:workspace-membership § Adding a member is a
manager-authored supersede`). No per-document rewrapping is needed — the new member can now
decrypt every document under the keyring. The indexer enforces the author's manager role
at write time.

**Removing a member.** A manager writes a supersede that rotates the group key and re-wraps
it to the remaining members, archiving the prior rotation's members into `keyHistory`
(§ 5a). The removed member is excluded from every re-wrap, so they cannot read anything
encrypted under the new key — forward secrecy is automatic. Historical documents stay
readable to remaining members through `keyHistory`; revoking that historical access
requires background re-encryption under a fresh content key.

## 5a. Keyring supersede (rotation after removing a member)

A keyring is the head of a supersede chain. The **genesis** keyring's rkey is a derived
tag — 26 lowercase-base32 characters — computed as
`base32-lower(SHA-256(Ed25519_pubkey(HKDF(K₀, transcript("opake-workspace-identity",
owner_did))))[..16])`, where `K₀` is the rotation-0 group key. Because the genesis URI is
the workspace identity, that identity commits to key material only members hold: a party
cannot mint a genesis keyring for a workspace whose rotation-0 key it lacks. Any adopting
client re-derives the tag from the record's rotation-0 key and rejects a mismatch, offline
and members-only — the indexer cannot, as it holds no group key
(`spec:workspace-identity § Identity adoption verifies by derivation`). The construction is
pinned as a wire-frozen KAT; see [CRYPTO.md](../docs/CRYPTO.md#workspace-identity). The
genesis URI used throughout these examples is the real derivation for a rotation-0 key of
`0x42` repeated 32 times under `did:plc:alice123` — reproducible against
`derive_workspace_identity_tag` (crates/opake-crypto):

```
at://did:plc:alice123/at.opake.keyring/6ywb55pjvb2xuzdbsy7udewfrq
```

When manager Bob removes Carol and rotates the group key, he writes a supersede on his own
PDS. It uses an ordinary client-generated TID rkey; `supersedes` points at the prior
canonical keyring, `supersedesCid` pins that predecessor's CID, and `lineage` carries the
genesis URI unchanged — the workspace identity never moves across a supersede.

```json
{
  "$type": "at.opake.keyring",
  "opakeVersion": 1,
  "algo": "aes-256-gcm",
  "members": [
    {
      "wrappedKey": {
        "did": "did:plc:alice123",
        "ciphertext": { "$bytes": "base64-rotation-1-group-key-for-alice" },
        "algo": "x25519-mlkem768-hkdf-a256kw-v2"
      },
      "role": "manager"
    },
    {
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
          "wrappedKey": {
            "did": "did:plc:alice123",
            "ciphertext": { "$bytes": "base64-rotation-0-group-key-for-alice" },
            "algo": "x25519-mlkem768-hkdf-a256kw-v2"
          },
          "role": "manager"
        },
        {
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
  "supersedes": "at://did:plc:alice123/at.opake.keyring/6ywb55pjvb2xuzdbsy7udewfrq",
  "supersedesCid": "bafyreib2xyz...cid-of-the-genesis-keyring",
  "lineage": "at://did:plc:alice123/at.opake.keyring/6ywb55pjvb2xuzdbsy7udewfrq",
  "createdAt": "2026-03-22T09:00:00.000Z"
}
```

- `keyHistory` retains rotation 0's members (minus Carol) so surviving members can still
  decrypt documents uploaded before the rotation. Carol's wrapped key is excluded — she
  cannot recover the old group key from this record (`spec:key-rotation § New members can
  read the full history they are admitted to` states the admission side of the same
  boundary).
- `supersedesCid` is required whenever `supersedes` is present. At v1 it is compared
  against the CID a serving host *reports* for the fetched predecessor — not a hash
  recomputed from bytes — so it catches CID disagreement between honest, non-colluding
  hosts (stale cache, accidental substitution) but is not yet a defense against a host
  serving tampered bytes under the true CID. Byte-level tamper-evidence is deferred to
  replication-tier work; the field is present now so that binding lands later without a
  wire migration.
- The same `supersedes` + `supersedesCid` pair rides every directory and document
  supersede with identical semantics: the pin always names the immediate predecessor and
  is never copied through a cascade.

## 6. Pair request (new device requesting identity)

A new device generates an ephemeral hybrid keypair (X25519 + ML-KEM-768) and publishes
both public halves. The X25519 fingerprint is displayed on both devices for out-of-band
comparison.

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

The record uses a TID rkey — several pending requests can coexist. The existing device
lists these to show what is waiting. The fingerprint is short enough to read aloud, long
enough to make collision search infeasible.

## 7. Pair response (existing device sending identity)

The existing device encrypts the full identity (X25519 + ML-KEM-768 + Ed25519 keypairs)
and wraps the content key to the ephemeral hybrid bundle from the request, using the same
hybrid construction as everywhere else (`spec:auth-pairing § Pairing wraps the full
identity to a device-held ephemeral keypair`).

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

**How the new device completes:**
1. Loads the ephemeral private bundle from local `Storage` (32 + 2400 bytes concatenated,
   keyed by DID + request rkey).
2. Unwraps the content key via the hybrid construction (X25519 ECDH + ML-KEM-768 Decaps +
   HKDF combiner).
3. Decrypts the ciphertext with the content key and nonce → identity JSON.
4. Verifies the embedded X25519 public key matches the sender's published `publicKey/self`
   record — the received identity is authenticated against the published key, not trusted
   on arrival (`spec:auth-pairing § Completion authenticates the received identity against
   the published key`).
5. Saves the identity to disk and wipes the pair state entry.

Both PDS records are deleted after a successful transfer (`spec:auth-pairing § Pair records
are relay ephemera, torn down after use`). The ephemeral private bundle is persisted in
`Storage` only between `create_pair_request` and `try_complete_pair` — it must survive a
CLI restart or browser reload while the user walks to the other device, so in-memory alone
will not do. It never crosses the WASM/JS boundary.

## 8. Pending share (recipient hasn't set up Opake yet)

Sharing with someone who has not published a public key writes a `pendingShare` instead of
a grant. The daemon retries until the recipient publishes their key, then promotes it to a
grant (`spec:sharing-grants § A share to a not-yet-ready recipient is queued, not
dropped`).

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

- `recipient` stores the handle or DID exactly as the user entered it — it need not be a
  resolved DID yet.
- `encryptedMetadata` holds `{ permissions, note }` encrypted with the document's content
  key — the same shape as a grant's metadata.
- There is no `wrappedKey`: the content key cannot be wrapped until the recipient publishes
  a public key. The daemon re-derives the content key from the document at retry time using
  the owner's identity.
- The daemon deletes the record after 7 days if the recipient never appears. Any device
  running the daemon can create or retry it.

## 9. Document supersede (collaborative editing)

An editor updates a document owned by another workspace member by writing a new
`at.opake.document` on their own PDS that supersedes the current one — there is no separate
update record and no owner round-trip. The indexer validates the supersede's authority at
write time and repoints the workspace snapshot at the new head.

```json
{
  "$type": "at.opake.document",
  "opakeVersion": 1,
  "blob": {
    "$type": "blob",
    "ref": { "$link": "bafkrei..." },
    "mimeType": "application/octet-stream",
    "size": 294912
  },
  "encryption": {
    "$type": "at.opake.document#keyringEncryption",
    "keyringRef": {
      "keyring": "at://did:plc:alice123/at.opake.keyring/6ywb55pjvb2xuzdbsy7udewfrq",
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
  "workspaceId": "at://did:plc:alice123/at.opake.keyring/6ywb55pjvb2xuzdbsy7udewfrq",
  "supersedes": "at://did:plc:alice123/at.opake.document/3kabcd",
  "supersedesCid": "bafyreie7...cid-of-the-superseded-document",
  "lineage": "at://did:plc:alice123/at.opake.document/3kgenesisdoc",
  "createdAt": "2026-03-21T10:00:00.000Z"
}
```

`supersedes` feeds the indexer's additive-advance authority check: an editor may advance a
chain but not fork or truncate it, while a manager is unrestricted (`spec:tree-chains §
Editor supersedes are additive; managers are unrestricted`). `supersedesCid` pins the
predecessor and `lineage` names the chain's genesis — the document's stable identity, to
which its content and metadata ciphertexts are AEAD-bound (`spec:document-crypto §
Ciphertexts are AAD-bound to their lineage anchor and type`). The same shape carries
document *adoption* when a member is removed: the substitute document supersedes the
departed member's original by URI.

## 10. Leaving a workspace

A member leaves by writing a keyring supersede on their own PDS that drops themselves from
`members`, carrying `lineage` unchanged. Leave is self-removal, permitted without manager
authority (`spec:workspace-membership § Keyring supersede authority is manager-only, except
pure self-removal`).

```json
{
  "$type": "at.opake.keyring",
  "opakeVersion": 1,
  "algo": "aes-256-gcm",
  "members": [
    {
      "wrappedKey": {
        "did": "did:plc:alice123",
        "ciphertext": { "$bytes": "base64-rotation-0-group-key-for-alice" },
        "algo": "x25519-mlkem768-hkdf-a256kw-v2"
      },
      "role": "manager"
    }
  ],
  "rotation": 0,
  "encryptedMetadata": {
    "ciphertext": { "$bytes": "base64-aes-256-gcm-encrypted-metadata-json" },
    "nonce": { "$bytes": "base64-encoded-12-byte-nonce" }
  },
  "supersedes": "at://did:plc:alice123/at.opake.keyring/3kpriorhead",
  "supersedesCid": "bafyreif0...cid-of-the-prior-head",
  "lineage": "at://did:plc:alice123/at.opake.keyring/6ywb55pjvb2xuzdbsy7udewfrq",
  "createdAt": "2026-03-21T11:00:00.000Z"
}
```

- Leaving deliberately does **not** rotate the group key: the leaver already holds it, so
  rotating buys no forward secrecy. Removal by a manager rotates; leave does not
  (`spec:workspace-membership § Removal rotates the group key; leave does not`). Forward
  secrecy against a departed member arrives with the next manager-authored rotation.
- Because the leaver's wrapped key is simply absent from the new head's `members`, they
  drop out of the workspace's member list once the indexer processes the supersede, and
  the workspace disappears from their sidebar.
- The last member cannot leave, and the only manager cannot leave without first promoting
  another member — the chain must never be orphaned (`spec:workspace-membership § Leave
  guards — no orphaned workspaces`).

## Design decisions

### Why encrypted metadata?

All document metadata — name, MIME type, size, tags, description — is encrypted inside
`encryptedMetadata` under the same content key as the blob. The PDS never sees a real
filename or tag. The cost is that server-side search and indexing would require
client-side decryption; the benefit is that no metadata leaks to the storage layer.

### Why separate grant records?

Recipients are not added to the document's `keys` array; each share is its own record.
That keeps the document untouched when sharing, lets a grant be deleted independently for
revocation, lets an indexer answer "what is shared with me?" across every document with one
query, and matches the atproto pattern of small, independent records.

### Why the two-layer key for keyrings?

A document under a keyring still has its own per-document content key, wrapped under the
group key rather than individual public keys. Rotating the group key therefore never
requires re-encrypting a single document's content, individual documents can be selectively
re-encrypted without touching the keyring, and the content key doubles as a per-document
domain separator for the group key.
