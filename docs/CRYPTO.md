# Opake — Cryptography Reference

Quick-reference for every algorithm, constant, and key type in the system. For the conceptual overview (why these choices, what the tradeoffs are), see [ARCHITECTURE.md](ARCHITECTURE.md).

All client-side cryptography lives in the [`opake-crypto`](../crates/opake-crypto/) crate — its own audit boundary, no I/O, no platform dependencies. Constants, types, and operations referenced below all sit there unless otherwise noted; `opake-core::crypto` is a re-export alias for backwards compatibility with existing call sites.

## Algorithms

| Name | Use | Library |
|------|-----|---------|
| AES-256-GCM | Content encryption (blobs + metadata) | `aes-gcm` |
| X25519 + ML-KEM-768 (HKDF-A256KW) | Hybrid asymmetric key wrapping | `x25519-dalek` + `libcrux-ml-kem` + `hkdf` + `aes-kw` |
| AES-256-KW (RFC 3394) | Symmetric key wrapping (content key → group key) | `aes-kw` |
| HKDF-SHA256 | KDF for key wrapping + identity derivation | `hkdf` + `sha2` |
| PBKDF2-HMAC-SHA512 | Mnemonic → master seed | `pbkdf2` + `sha2` |
| Ed25519 | Indexer authentication and account public-key signatures | `ed25519-dalek` |
| BIP-39 | 24-word mnemonic encoding (256-bit entropy) | `bip39` (embedded wordlist) |

The hybrid construction is `x25519-mlkem768-hkdf-a256kw-v2`. Aligned with [BSI TR-02102](https://www.bsi.bund.de/EN/Themen/Unternehmen-und-Organisationen/Standards-und-Zertifizierung/Technische-Richtlinien/TR-nach-Thema-sortiert/tr02102/tr02102_node.html) (Germany) and [ANSSI](https://www.ssi.gouv.fr/) guidance for hybrid post-quantum key establishment. ML-KEM-768 byte sizes follow [NIST FIPS-203](https://csrc.nist.gov/pubs/fips/203/final).

## Constants

```rust
HYBRID_WRAP_ALGO         = "x25519-mlkem768-hkdf-a256kw-v2"
CONTENT_KEY_LEN          = 32      // 256 bits (AES-256)
AES_GCM_NONCE_LEN        = 12      // 96 bits (standard for AES-GCM)
X25519_KEY_LEN           = 32      // 256 bits (Curve25519)
ML_KEM_PK_LEN            = 1184    // FIPS-203 §6.1
ML_KEM_SK_LEN            = 2400    // FIPS-203 §6.2
ML_KEM_CT_LEN            = 1088    // FIPS-203 §6.2
ML_KEM_SS_LEN            = 32      // FIPS-203 §6.2
AES_KW_OVERHEAD          = 8       // RFC 3394 integrity check
WRAPPED_KEY_LEN          = 40      // 32 (content key) + 8 (AES-KW overhead)
HYBRID_CIPHERTEXT_LEN    = 1160    // 32 (X25519 eph pub) + 1088 (ML-KEM ct) + 40 (AES-KW)

PBKDF2_ROUNDS            = 2048    // BIP-39 standard
PBKDF2_SALT              = b"mnemonic"  // BIP-39 standard (no passphrase)
SCHEMA_VERSION           = 1       // Embedded in HKDF info strings

IDENTITY_TAG_LEN         = 16      // 128-bit pubkey-hash commitment in the genesis rkey
```

Transcript labels domain-separate five consumers: `opake-wrap-info` (key wraps),
`opake-seal-aad` (content/metadata AAD), `opake-workspace-identity` (genesis rkey derivation),
`at.opake.publicKey/self:v<n>` (account public-key signatures), and
`at.opake.unverified-key-approval:v<n>` (unverified-recipient approval). Each consumer family has
a distinct label; its declared version is encoded as a separate framed field where applicable.

## Key Types

| Type | Size | Rust type | Where stored |
|------|------|-----------|-------------|
| Content key | 32 bytes | `ContentKey([u8; 32])` | Wrapped in document record (never plaintext on PDS) |
| Group key | 32 bytes | `ContentKey([u8; 32])` | Wrapped per-member in keyring record. Plaintext stored locally in `keyrings/<rkey>.json` |
| X25519 public key | 32 bytes | `X25519PublicKey = [u8; 32]` | Published as `publicKey/self` record on PDS (`x25519PublicKey` field) |
| X25519 private key | 32 bytes | `X25519PrivateKey = [u8; 32]` | `identity.json` (local only, 0600 permissions) |
| ML-KEM-768 public key | 1184 bytes | `MlKemPublicKey = [u8; 1184]` | Published as `publicKey/self` record on PDS (`mlKemPublicKey` field) |
| ML-KEM-768 private key | 2400 bytes | `MlKemPrivateKey = [u8; 2400]` | `identity.json` (local only) |
| Ed25519 signing key | 32 bytes | `Ed25519SecretKey = [u8; 32]` | `identity.json` (local only) |
| Ed25519 verify key | 32 bytes | `Ed25519VerifyKey = [u8; 32]` | Published as `signingKey` on `publicKey/self` record |
| DPoP key | P-256 | `DpopKeyPair` (JWK) | `session.json` (per-session, not per-identity) |

X25519 and ML-KEM-768 keys travel together in `PublicKeyBundle<'a>` and `PrivateKeyBundle<'a>` views. `Identity::owned_public_keys()` / `owned_private_keys()` return owned (zeroizing) wrappers that own the bytes; their `bundle()` accessor borrows into the view.

## Key Hierarchy

```
Seed phrase (24 words / 256-bit entropy)
  │
  ├─ PBKDF2-HMAC-SHA512(salt="mnemonic", rounds=2048)
  │  → 512-bit master seed
  │     │
  │     ├─ HKDF-SHA256(info="opake-v1-x25519-identity")  → 32 bytes
  │     │  → X25519 private key → X25519 public key (published)
  │     │
  │     ├─ HKDF-SHA256(info="opake-v1-mlkem768-keygen")  → 64 bytes
  │     │  → seed for libcrux ML-KEM-768 keygen (FIPS-203 §7.1)
  │     │  → ML-KEM-768 private + public key (published)
  │     │
  │     └─ HKDF-SHA256(info="opake-v1-ed25519-signing")  → 32 bytes
  │        → Ed25519 signing key → Ed25519 verify key (published)
  │
  └─ Same phrase always produces the same keys (deterministic)


Content key (per-document, random)
  │
  ├─ Direct encryption: hybrid-wrapped to each authorized DID's public-key bundle
  │  → stored in document.encryption.envelope.keys[]
  │
  └─ Keyring encryption: wrapped under group key (AES-KW)
     → stored in document.encryption.keyringRef.wrappedContentKey

Group key (per-keyring, random)
  │
  └─ Hybrid-wrapped to each member's public-key bundle
     → stored in keyring.members[].wrappedKey
     → plaintext stored locally in keyrings/<rkey>.json (per-rotation)
```

## Workspace Identity

A workspace's identity is its genesis keyring URI. That URI is not an arbitrary address the creator picks — its rkey is **derived from the genesis group key and the owner DID**, so the identity commits to key material only members hold. A party cannot mint a keyring that claims a workspace whose rotation-0 key it does not possess.

### Genesis rkey derivation (`identity_tag.rs`)

```
seed  = HKDF-SHA256(ikm = K₀,                       [K₀ = rotation-0 group key]
                    info = transcript("opake-workspace-identity", [owner_did]))
                    [..32 bytes]
(sk, pk) = Ed25519 keygen(seed)
rkey  = base32-lower(SHA-256(pk)[..16])             [26 chars, rkey-charset-safe]
uri   = at://<owner_did>/at.opake.keyring/<rkey>
```

- **The owner DID is folded into the derivation.** A tag is therefore valid under exactly one authority: a forged rkey fails on the key, a forged owner attribution fails on the DID, and both halves of the URI are checked in one offline derivation with no repo lookup.
- **The rkey commits to a public key, not a bare KDF tag.** At v1 nothing is signed and the private half is used nowhere — but the commitment means workspace *signatures* (verifiable by non-members via `(pk, sig)` plus the rkey pin, no key registry) can land later as an additive field rather than an identity migration. This is the seam replication/archival verification will build on.
- **KAT-pinned.** `derive_workspace_identity_tag(key=0x42×32, "did:plc:pinned") == "452upqgt6ql7ci462dvsfcv6bm"`. The construction is a wire-frozen identity — every existing workspace's URI *is* its derived tag — so a drift in the label, transcript, keygen, hash, or encoding fails the KAT loudly rather than silently orphaning every workspace.
- **Zeroization.** The HKDF `seed` is `Zeroizing`; the Ed25519 `SigningKey` uses `ed25519-dalek`'s `zeroize` feature and is dropped immediately after the public key is taken. The private half never escapes the derivation.

Creation order dissolves the circularity that sinks content-hash rkeys: `K₀ → keypair → tag → URI`, and only then do the member wraps and metadata AAD bind that URI. The identity precedes everything it anchors.

### Adoption verification (`workspace.rs::verify_workspace_identity`)

Every path that keys workspace state under a *declared* identity re-derives the tag from the record's rotation-0 key and the declared anchor's authority DID, and compares it to the anchor's rkey. This is one offline KDF — no network, no chain walk, no persisted state, cost independent of chain length. The adopting paths are enumerated as a contract, because a single unguarded path reopens the attack:

- **Direct resolution** — `resolve_workspace_by_uri` (both branches), `resolve_foreign_workspace`. Mismatch → distinct `WorkspaceIdentityMismatch` error.
- **Web keeper** — `WorkspaceKeeper` bootstrap + `keyring:upsert` patch (`try_build_entry`). Mismatch → **silent drop** (no entry, no placeholder, no signal — a rendered artifact is the forger's payoff).
- **CLI daemon sync** — `sync_single_workspace`. The CLI has no keepers, so this loop is its sole adoption surface. Mismatch → sync error, adopts nothing.

**Threat model.** An outsider holding no workspace key material cannot construct a keyring that resolves as another workspace — it would require a preimage of the victim's tag. Key-holders (members and ex-members) *can* mint identity-valid records; forks by key-holders are the jurisdiction of chain-authority enforcement (the indexer's write-time gates + the client chain-walk mirror), and the set of parties that can forge a workspace's identity is exactly the set already trusted with its contents. The check is client-side and members-only: the indexer cannot run it (it holds no group key), which is by design — identity adoption is a client decision about client state.

## Supersede Content Pin

Every superseding record (keyring, document, directory) carries `supersedesCid` alongside `supersedes` — the CID of the immediate predecessor it supersedes, stamped by the writer from the chain-head pointer it holds. It names the immediate predecessor only and is never copied through verbatim-copy paths; each level of a cascade pins its own predecessor.

**v1 verification scope — reported CID, not recomputed hash.** The chain walk (`directories/chain.rs`) compares the pin against the CID the serving host *reports* for the fetched predecessor. Clients do not compute atproto CIDs (canonical dag-cbor + multihash) today, so the pin detects disagreement between honest, non-colluding hosts — a stale cache, an accidental substitution, an indexer/PDS reporting inconsistent heads — but **not** a malicious host that serves tampered bytes while reporting the true CID, which controls both sides of the comparison. Real byte-level tamper-evidence requires recomputing the CID from the fetched bytes; that is deferred to the replicated/archival serving work that needs it (issue #64), where records arrive from untrusted third parties.

The pin is **not** what protects workspace identity — derivation is — and it is not, at v1, a trust boundary against a hostile host. It is defense-in-depth against honest-host CID inconsistency, and a pre-v1 wire reservation so the byte-binding can be added as an already-present field rather than a post-freeze migration. A CID disagreement classifies the link unverifiable: directory chains degrade to the newest fully-verifiable head, authority walks reject the proposed head.

## Operations

### Context transcripts (`transcript.rs`)

Every byte string that commits to a tuple of context fields — the HKDF `info` for key wraps, AAD
for content/metadata encryption, account signatures, and approval commitments — is produced by one
injective encoder:

```
transcript(label, fields)
  → label ‖ u32le(field_count) ‖ (u32le(len) ‖ bytes) per field
```

Length-prefixing makes the encoding injective by construction: no arrangement of field contents can imitate another arrangement. (Delimiter-joining is not injective when a field may contain the delimiter — `did:web` identifiers legally contain hyphens.) Every consumer uses a distinct label, so a transcript for one purpose cannot collide with a transcript for another.

### Account public-key signatures

An account that publishes an `#opake` verification method in its DID document signs its
`at.opake.publicKey/self` record with the mnemonic-derived Ed25519 key. The signature transcript
uses `at.opake.publicKey/self:v<n>` and this fixed ordered tuple:

```
[did, opakeVersion_le32, x25519PublicKey, x25519Algo,
 mlKemPublicKey, mlKemAlgo, signingKey, signingAlgo, createdAt]
```

The key bytes are decoded before encoding the tuple. `signature` and `signatureAlgo` are excluded,
as are fields added after this scheme version, so JSON re-serialization and future additive fields
cannot invalidate a valid signature. Verifiers select the scheme from the record's `opakeVersion`
and the signature algorithm from its declared `signatureAlgo`; they verify against the DID
document's `#opake` key rather than trusting the record's own `signingKey`. The DID in the tuple
prevents copying a signed record to a different account.

The signature binds a key record to the DID document that exists at resolution
time. It does not constrain a party that can replace that document's `#opake`
method. Deployments that need independence from the PDS or another service
holding that authority keep a PLC rotation key outside that service and give it
higher DID authority. That key can replace a compromised service key or nullify
a replacement while the PLC recovery policy permits it; it cannot recover keys
or plaintext that a recipient already obtained, and it cannot help after every
independent PLC rotation credential is lost.

### Approval for unverified recipient keys

When an owner or manager explicitly accepts an unverified recipient's encryption bundle, the
relationship record stores a 32-byte SHA-256 commitment rather than a timestamp-sensitive copy of
the whole public-key record:

```
SHA-256(transcript("at.opake.unverified-key-approval:v<n>",
  [relationship_scope_uri, recipient_did, x25519PublicKey, x25519Algo,
   mlKemPublicKey, mlKemAlgo]))
```

The scope is a workspace's genesis URI for membership or a document URI for a share. The enclosing
relationship record's `opakeVersion` supplies `<n>`. Changing either encryption key or either
algorithm therefore needs a new decision, while an unchanged bundle keeps approval across a new
timestamp or JSON encoding.

### Seal contexts — AAD (`seal_context.rs`)

Every AES-256-GCM ciphertext is bound to a `SealContext` as associated data: the **lineage anchor** of the record it belongs to (the chain's genesis URI — the record's own URI when it is genesis or never chains) and the **seal type** of the field it seals.

```
SealContext { anchor, seal_type }
  → AAD = transcript("opake-seal-aad", [anchor, seal_type])
```

| Ciphertext | Anchor | Seal type |
|---|---|---|
| Document blob | document's lineage anchor | `document-blob` |
| Document metadata | document's lineage anchor | `document-metadata` |
| Keyring metadata | workspace genesis (keyring lineage anchor) | `keyring-metadata` |
| Directory metadata | directory's lineage anchor | `directory-metadata` |
| Pending-share metadata | *target document's* URI | `grant-metadata` |
| Pairing identity blob | `self:pair-response` sentinel | `pair-identity` |

The type tag matters because one content key seals both a document's blob and its metadata: without AAD, swapping those ciphertexts inside a record decrypts cleanly and only fails if the bytes don't parse. The anchor is chain-constant, so a ciphertext copied verbatim into a superseding record (keyring metadata on an advance, directory metadata through a cascade) still authenticates — the binding names the object, not the record.

### Content encryption (`content.rs`)

```
encrypt_blob(content_key, plaintext, seal_context, rng)
  → AES-256-GCM(key=content_key, nonce=random_96bit, plaintext,
                aad=seal_context.aad())
  → { ciphertext, nonce }

decrypt_blob(content_key, { ciphertext, nonce }, seal_context)
  → AES-256-GCM decrypt with the same AAD
  → plaintext
```

Nonce is 12 bytes, generated fresh per encryption. The PDS blob is the raw ciphertext — no framing or headers; the AAD travels nowhere, the reader reconstructs it from the record it fetched.

### Hybrid asymmetric key wrapping (`key_wrapping.rs`)

```
wrap_key(content_key, recipient: &PublicKeyBundle, recipient_did, rng)
  ── Classical half ──────────────────────────────────────────────
  → ephemeral_secret = X25519 random
  → ephemeral_pubkey = X25519 public from ephemeral_secret
  → x25519_shared   = ECDH(ephemeral_secret, recipient.x25519)
  ── Post-quantum half ──────────────────────────────────────────
  → validate recipient.ml_kem  (FIPS-203 §7.2 — fails on bad keys)
  → encap_randomness = 32 random bytes
  → (mlkem_ct, mlkem_shared) = ML-KEM-768 Encaps(recipient.ml_kem,
                                                 encap_randomness)
  ── Combiner ──────────────────────────────────────────────────
  → salt = ephemeral_pubkey ‖ recipient.x25519 ‖ mlkem_ct
  → ikm  = x25519_shared ‖ mlkem_shared              [64 bytes]
  → wrapping_key = HKDF-SHA256(
       extract_salt = salt,
       ikm          = ikm,
       expand_info  = transcript("opake-wrap-info",
                        [version_le32, algo, context_tag, context_uri,
                         recipient_did]),
       length       = 32
     )
  ── AES-KW around the content key ─────────────────────────────
  → wrapped = AES-256-KW(key=wrapping_key, plaintext=content_key)
  → ciphertext = ephemeral_pubkey ‖ mlkem_ct ‖ wrapped   [1160 bytes]
  → WrappedKey { did, ciphertext, algo = "x25519-mlkem768-hkdf-a256kw-v2" }
```

The HKDF salt commits to both the recipient's published X25519 pubkey and the ML-KEM ciphertext. An attacker who can flip or substitute the post-quantum half breaks the AES-KW integrity check at the recipient — the construction is "splice-resistant" in the sense of [Bindel et al., "Hybrid Key Encapsulation Mechanisms and Authenticated Key Exchange" (PQCrypto 2019)](https://eprint.iacr.org/2018/903).

```
unwrap_key(wrapped_key, keys: &PrivateKeyBundle)
  → reject if wrapped_key.algo != "x25519-mlkem768-hkdf-a256kw-v2"
  → split ciphertext: eph_pub [0..32], mlkem_ct [32..1120], wrapped [1120..1160]
  → x25519_shared = ECDH(keys.x25519, eph_pub)
  → mlkem_shared  = ML-KEM-768 Decaps(keys.ml_kem, mlkem_ct)
  → recipient_pub = X25519::from(keys.x25519)         [for transcript]
  → salt = eph_pub ‖ recipient_pub ‖ mlkem_ct
  → wrapping_key = HKDF-SHA256(same params as wrap)
  → content_key = AES-256-KW unwrap(wrapping_key, wrapped)
```

The recipient derives its own X25519 public key from its private key rather than trusting the envelope to carry it redundantly. That keeps the salt's integrity tied to the recipient's identity, not to whatever bytes the envelope happens to contain.

### Symmetric key wrapping (`keyring_wrapping.rs`)

```
wrap_content_key_for_keyring(content_key, group_key)
  → AES-256-KW(key=group_key, plaintext=content_key)
  → 40 bytes (32 key + 8 integrity)

unwrap_content_key_from_keyring(wrapped, group_key)
  → AES-256-KW unwrap
  → content_key
```

No HKDF, no ephemeral keys — pure symmetric wrapping. The group key IS the KEK. (No post-quantum upgrade needed — the secret never leaves a small, controlled set of devices.)

### Group key creation (`key_wrapping.rs`)

```
create_group_key(members: &[DidMember], rng)
  → group_key = random 256 bits
  → for each member:
      wrap_key(group_key, member.public_keys(), member.did, rng)
  → (group_key, Vec<WrappedKey>)
```

`DidMember` carries both halves of the recipient's public key alongside their DID; `public_keys()` returns the matching `PublicKeyBundle` view for `wrap_key`. Callers pair the returned `WrappedKey`s with roles to build `KeyringMember` entries — the crypto layer doesn't know about roles.

### Metadata encryption (`metadata.rs`)

```
encrypt_metadata<T: Serialize>(key, metadata, seal_context, rng)
  → json = serde_json::to_vec(metadata)
  → AES-256-GCM(key, json, random_nonce, aad=seal_context.aad())
  → EncryptedMetadata { ciphertext: base64, nonce: base64 }

decrypt_metadata<T: DeserializeOwned>(key, encrypted, seal_context)
  → decode base64 ciphertext + nonce
  → AES-256-GCM decrypt with the same AAD
  → serde_json::from_slice → T
```

Same AES-256-GCM as blob encryption but with JSON serialization. The key depends on context:
- **Documents/grants**: content key (same key that encrypted the blob)
- **Keyrings**: group key (so all members can read the name)
- **Directories**: content key (from the directory's key wrapping)

## Metadata Types

| Record | Metadata type | Key used | Fields |
|--------|--------------|----------|--------|
| Document | `DocumentMetadata` | Content key | name, mimeType?, size?, tags[], description? |
| Keyring | `KeyringMetadata` | Group key | name, description? |
| Grant | `GrantMetadata` | Content key | permissions?, note? |
| Directory | `DirectoryMetadata` | Content key | name, description? |

All metadata is always encrypted — there is no plaintext mode. Record-level fields like `name` on the PDS are dummies (`"encrypted"`, `"application/octet-stream"`).

## Record Structure

### WrappedKey (crypto primitive)

```json
{
  "did": "did:plc:alice",
  "ciphertext": { "$bytes": "<1160 bytes base64>" },
  "algo": "x25519-mlkem768-hkdf-a256kw-v2"
}
```

Used everywhere: document encryption envelopes, grants, group keyring members, pair-flow responses. The 1160-byte ciphertext layout is `eph_x25519_pub (32) ‖ ml_kem_ct (1088) ‖ aes_kw_wrapped (40)`.

### KeyringMember (domain type)

```json
{
  "did": "did:plc:alice",
  "role": "manager"
}
```

`wrappedKey` and `unverifiedKeyApproval` are optional. A present `wrappedKey`
has the same `{ did, ciphertext: { "$bytes": "..." }, algo }` shape as the
wrapped-key example above and must name this member's DID. A present
`unverifiedKeyApproval` is `{ "$bytes": "<32-byte base64 commitment>" }`.

Only used in keyring `members` and `keyHistory` arrays. A member is an
explicit `(did, role)` relationship; `wrappedKey` is optional key material for
that relationship, not the relationship itself. A listed member with no
current wrap remains admitted and may retain usable historical wraps, but
cannot decrypt current-generation metadata or create new current-generation
content until a manager repairs that wrap. Every member list, including a
history snapshot, has unique DIDs; a present wrap must name its containing
member's DID.

`unverifiedKeyApproval`, when present, is a 32-byte commitment to the
relationship version, workspace genesis URI, member DID, and the exact hybrid
encryption keys and algorithms. It is not a general consent flag. A manager
may re-wrap to an unverified account without another prompt only when a fresh
resolution produces the same commitment. A changed key, missing approval, or
verification failure leaves the member admitted without a new wrap; it never
copies an old generation's wrap into the current slot.

The member representation is a pre-v1 structural reset: development fixtures
and local dev databases must be reset together before using this draft. There
is no legacy reader, inferred DID, or inferred approval. This reset procedure
is intentionally unavailable after the v1 protocol freeze; later structural
changes require a new collection NSID.

Pending shares bind their resolved recipient DID and their explicit
first-publication permission inside encrypted intent metadata. Completion
creates the designated grant and consumes that unchanged intent in one
same-repository CAS transaction. A retry never transfers the permission to a
different DID or publishes a second grant after a conflict.

### Encryption union on documents

```
directEncryption:
  envelope.algo = "aes-256-gcm"
  envelope.nonce = 12 bytes
  envelope.keys = [WrappedKey, ...]    ← content key wrapped to each authorized DID

keyringEncryption:
  keyringRef.keyring = AT-URI of keyring record
  keyringRef.wrappedContentKey = 40 bytes   ← content key wrapped under group key (AES-KW)
  keyringRef.rotation = integer             ← which generation of the group key
  algo = "aes-256-gcm"
  nonce = 12 bytes
```

### KeyWrapping union on directories

Directories have no blob — only encrypted metadata. They use `KeyWrapping` instead of `Encryption` to carry just the key material, without the blob-specific `algo` and `nonce` fields.

```
directKeyWrapping:
  keys = [WrappedKey, ...]               ← content key wrapped to each authorized DID

keyringKeyWrapping:
  keyringRef.keyring = AT-URI of keyring record
  keyringRef.wrappedContentKey = 40 bytes ← content key wrapped under group key (AES-KW)
  keyringRef.rotation = integer           ← which generation of the group key
```

The content key encrypts `encryptedMetadata` (which carries its own nonce internally). Decryption: unwrap content key → `decrypt_metadata(key, encrypted_metadata)`.

Personal directories use `directKeyWrapping`. Workspace directories use `keyringKeyWrapping` — all workspace members who can unwrap the group key can read and propose changes to the directory structure.

## Device Pairing

Pairing a fresh device to an existing identity uses the same hybrid construction as everywhere else. The new device generates an ephemeral hybrid keypair (X25519 + ML-KEM-768), publishes both public halves in `at.opake.pairRequest`, and the responding device wraps the existing identity to that bundle via `wrap_key`. The `pairResponse` record carries a single 1160-byte `WrappedKey` and an AES-256-GCM ciphertext of the serialized `Identity`.

The new device's ephemeral private bundle is persisted to local Storage (32 + 2400 = 2432 bytes, X25519 ‖ ML-KEM-768) under `(did, request_rkey)` so it survives between request and response — the response can take minutes to days to arrive.

## Key Rotation

### The rotation event

On member removal, the group key rotates inside the single operation that triggers it:

1. Archive current `members` array into `keyHistory` (minus the removed member)
2. Generate new group key
3. Hybrid-wrap the new group key to each remaining member's `PublicKeyBundle` (preserving their roles)
4. Increment `rotation` counter
5. Re-encrypt keyring metadata under new group key

This is **one bounded write with no blob work** — per-document content keys are wrapped under the group key precisely so rotating the group key never re-encrypts ciphertext (key hierarchy, above). The moment the keyring supersede lands, the workspace is fully correct: forward secrecy holds against the removed member, and every remaining member can still read every document. Nothing else has to run — not the sweep below, not any background task. (The lifecycle contract for this is the `key-rotation` capability under `openspec/specs/`.)

Adding a member does **not** rotate. The admitting manager wraps the current group key *and* every retained `keyHistory` key to the joiner, so a member admitted after N rotations can still read documents written under all N prior generations. You cannot grant a rotation the admitting manager no longer holds.

### Rotation-selected reads and key history

Documents encrypted under an old group key remain readable — a reader looks up the document's `keyringRef.rotation` in `keyHistory` to recover the wrapped group key of that generation. New documents use the current key. `keyHistory` grows by one retained key per rotation; readers walk it, so an unswept workspace pays a proportionally longer walk. That is a **performance cost only, never a correctness cliff** — a historical key is never pruned while any live document's wrap still references its rotation.

Live clients adopt a rotation from the SSE event itself — the new key is unwrapped straight from the keyring record, the prior key is archived, and cached directory names re-decrypt in place. No reload, re-login, or re-bootstrap is required to keep reading after a rotation.

### The re-wrap sweep — and what rotation does *not* do

Rotation is **forward secrecy only.** It stops the removed member from reading content created *after* the rotation. It does **not** retroactively revoke content they already fetched or could have unwrapped while they were a member — the same posture grants carry (`sharing-grants`). There is no cryptographic take-back of data a device already held.

The re-wrap sweep migrates existing documents' content-key wraps from historical group keys to the current one. Its **only** effect is to bound the `keyHistory` walk readers perform — **it has no security effect whatsoever.** A re-wrap cannot revoke anything a former member could already unwrap; the content key it re-wraps is byte-for-byte the same key. Anyone who believes the sweep "completes" revocation has the model backwards: revocation is what rotation already did (forward), and the sweep is pure hygiene.

The sweep is therefore a background task under the [background-work contract](BACKGROUND_WORK.md): its work set is derived from records (documents whose `keyringRef.rotation` trails the head), each re-wrap is a single CAS-conditioned write, duplicate or interrupted runners re-derive the remainder, and completion is never required for any guarantee. `crates/opake-core/src/rewrap.rs` holds the primitive; `Opake::sweep_owned_documents_rewrap` orchestrates it; the CLI daemon drains it and the web tier runs it opportunistically.

Old group keys are also cached locally per-rotation in `keyrings/<rkey>.json`:
```json
{ "keys": [{ "rotation": 0, "group_key": "base64..." }, { "rotation": 1, "group_key": "base64..." }] }
```

## RNG Injection

All crypto functions take `&mut (impl CryptoRng + RngCore)` — no global `OsRng`. This makes the crypto layer:
- Testable (inject a deterministic RNG)
- WASM-compatible (`OsRng` works in browsers via `getrandom` + Web Crypto)
- Explicit about where randomness enters the system

Production callers pass `&mut OsRng`. The WASM layer does the same — `OsRng` in wasm32 delegates to `crypto.getRandomValues()`. ML-KEM-768 KeyGen consumes 64 bytes of randomness (FIPS-203 §7.1); Encaps consumes 32 bytes (FIPS-203 §7.2).

## Memory Safety

Sensitive types are zeroized on drop to prevent key material lingering in memory:

- **`RedactedDebug` derive macro** (opake-derive) generates three impls for structs with `#[redact]` fields:
  1. `Debug` — shows `[N bytes]` instead of content
  2. `Zeroize` — overwrites redacted fields with zeros
  3. `Drop` — calls `zeroize()` automatically

- **Types with automatic zeroization:**
  - `ContentKey` — AES-256 content encryption key
  - `Identity` — `x25519_private_key`, `ml_kem_private_key`, `signing_key` fields (base64 strings zeroed)
  - `OwnedPrivateKeys` — owned X25519 + ML-KEM private bytes (`Zeroizing` wrappers)
  - `DpopKeyPair` — `private_key_b64` (P-256 private key for DPoP proof generation)
  - `LegacySession` — `access_jwt`, `refresh_jwt`
  - `OAuthSession` — `access_token`, `refresh_token` (nested `DpopKeyPair` chains zeroization)
  - `Cabinet` — raw X25519 + ML-KEM private key bytes (explicit `#[derive(Zeroize, ZeroizeOnDrop)]`)
  - `Workspace` — workspace key / `ContentKey` (explicit `#[derive(Zeroize, ZeroizeOnDrop)]`)

- `ContentKey` is intentionally NOT `Copy` — `Copy` types can be implicitly duplicated, escaping zeroization. `Clone` requires explicit intent.

- **Workspace-identity derivation intermediates** (`identity_tag.rs`) — the HKDF `seed` is `Zeroizing<[u8; 32]>` and the derived Ed25519 `SigningKey` (zeroized via `ed25519-dalek`'s `zeroize` feature) is dropped immediately after its public key is taken. The derivation runs on every identity adoption, so these are materialized far more often than seed-phrase-derived keys; the private half is used by no operation and never leaves the function.

- `PrivateKeyBundle<'a>` and `PublicKeyBundle<'a>` are *views*, not owners. They borrow into a longer-lived owner (`Identity`, `Cabinet`, `OwnedPrivateKeys`); their `Debug` impl prints byte-length only via the `Redacted` adapter.

## Security Properties

| Property | Guaranteed | Mechanism |
|----------|-----------|-----------|
| Confidentiality (content) | Yes | AES-256-GCM, client-side only |
| Confidentiality (metadata) | Yes | Same content key encrypts metadata |
| Integrity (content) | Yes | GCM authentication tag |
| Integrity (wrapped keys) | Yes | AES-KW integrity check (8-byte overhead) |
| Forward secrecy (new content) | Yes | Group key rotation on member removal |
| Forward secrecy (old content) | No | Removed member may have cached plaintext |
| Key independence | Yes | Ephemeral ECDH per wrapping, HKDF domain separation |
| Deterministic recovery | Yes | BIP-39 mnemonic → same X25519, ML-KEM, Ed25519 keys every time |
| Post-quantum confidentiality | Yes (IND-CCA2) | ML-KEM-768 in the hybrid combiner — breaking the wrap requires breaking *both* X25519 and ML-KEM |
| Post-quantum authenticity | No (deferred) | Ed25519 signing keys are still classical; record signatures by atproto's existing scheme |
| Workspace identity unforgeability | Yes (to non-key-holders) | Genesis rkey derived from the group key + owner DID; forging it requires the rotation-0 key (a preimage otherwise). Adoption re-derives and rejects mismatches. Key-holders are not excluded — see the threat model under *Workspace Identity* |
| Chain integrity vs a hostile host | No (v1) | The supersede pin compares reported CIDs, not recomputed hashes — honest-host disagreement only. Byte-binding deferred to #64 |

The hybrid construction provides "harvest-now-decrypt-later" resistance: an adversary recording today's ciphertexts cannot decrypt them with a future quantum computer unless ML-KEM-768 is broken in the meantime. Per BSI TR-02102 / ANSSI, hybrid is the recommended deployment posture for transitional security — the combiner is at least as strong as either component (Bindel-Brendel-Fischlin-Goncalves-Stebila, PQCrypto 2019).

## Nonce Collision Blast Radius

AES-256-GCM is catastrophically broken if the same (key, nonce) pair is reused — the attacker can recover the GCM auth subkey and the XOR of the two plaintexts. Per-document content keys limit the blast radius:

| Key | Used with AES-GCM | Nonces per lifetime | Collision blast radius |
|-----|-------------------|---------------------|----------------------|
| Per-document content key | Blob + metadata encryption | 2 (one blob, one metadata) | One document |
| Group key (keyring) | Keyring metadata only | N (one per keyring record update) | Keyring name/description disclosure |
| Group key (keyring) | Content key wrapping | 0 — AES-KW is nonceless | N/A |

**Documents are always safe.** Even under keyring encryption, each document has its own random content key. A nonce collision between two documents encrypted under the same keyring cannot happen — different keys.

**Group key exposure is limited to keyring metadata.** The group key encrypts the keyring's own name/description via AES-GCM, re-encrypted on each member change. A nonce collision here (astronomically unlikely for 96-bit nonces with low N) would leak the keyring name — not document contents, not the group key itself. GCM nonce reuse compromises the authentication subkey (H), not the encryption key.

**Content key wrapping is nonceless.** AES-KW (RFC 3394) is a deterministic algorithm — no nonce, no IV. The group key wraps content keys without any nonce exposure.

The 96-bit random nonce gives a collision probability of ~2^-32 after 2^32 encryptions under the same key (birthday bound). With 2 nonces per document and N nonces per keyring lifetime, this is not a practical concern — but the per-document key design means even a theoretical collision can't cascade.

### Why not derived nonces or AES-GCM-SIV?

Two alternatives exist that eliminate nonce collision risk entirely:

**Derived nonces** (e.g. HKDF from content hash + counter): deterministic, no collision possible. Downsides:
- Requires reading the full plaintext before encryption can start (can't stream)
- Adds a hash pass over the data — slow for large files (photos, videos)
- HKDF per-encryption adds latency on every operation
- Makes the encryption non-randomized — identical plaintext + key produces identical ciphertext, which leaks equality

**AES-256-GCM-SIV** (RFC 8452): derives the tag from the message content, so nonce reuse degrades gracefully — leaks plaintext equality only, doesn't expose the auth key. Same key/nonce/tag sizes as AES-GCM, same `Aead` trait in Rust.

We chose random-nonce AES-256-GCM for v1 because:
1. Per-document keys make the birthday bound irrelevant (2 nonces per key, not 2^32)
2. `OsRng` via Web Crypto / `getrandom` is solid on all target platforms
3. AES-GCM has universal hardware acceleration and library support
4. Streaming encryption without a pre-read pass matters for large uploads

AES-GCM-SIV is planned for `SCHEMA_VERSION` v2 as a cipher swap. The migration path: version-gated decrypt (v1 = AES-GCM, v2 = AES-GCM-SIV), SIV-only encrypt going forward. Existing v1 records remain readable. No key derivation changes, no identity migration — just a cipher swap with a proactive re-encryption command for users who want to upgrade old records.
