# Opake — Cryptography Reference

Quick-reference for every algorithm, constant, and key type in the system. For the conceptual overview (why these choices, what the tradeoffs are), see [ARCHITECTURE.md](ARCHITECTURE.md).

## Algorithms

| Name | Use | Library |
|------|-----|---------|
| AES-256-GCM | Content encryption (blobs + metadata) | `aes-gcm` |
| X25519-HKDF-A256KW | Asymmetric key wrapping (content key → recipient pubkey) | `x25519-dalek` + `hkdf` + `aes-kw` |
| AES-256-KW (RFC 3394) | Symmetric key wrapping (content key → group key) | `aes-kw` |
| HKDF-SHA256 | KDF for key wrapping + identity derivation | `hkdf` + `sha2` |
| PBKDF2-HMAC-SHA512 | Mnemonic → master seed | `pbkdf2` + `sha2` |
| Ed25519 | AppView authentication signatures | `ed25519-dalek` |
| BIP-39 | 24-word mnemonic encoding (256-bit entropy) | `bip39` (embedded wordlist) |

`x25519-hkdf-a256kw` is intentionally distinct from JWE's `ECDH-ES+A256KW` — we use HKDF-SHA256, not JWE's Concat KDF.

## Constants

```rust
WRAP_ALGO         = "x25519-hkdf-a256kw"
CONTENT_KEY_LEN   = 32      // 256 bits (AES-256)
AES_GCM_NONCE_LEN = 12      // 96 bits (standard for AES-GCM)
X25519_KEY_LEN    = 32      // 256 bits (Curve25519)
AES_KW_OVERHEAD   = 8       // RFC 3394 integrity check
WRAPPED_KEY_LEN   = 40      // 32 (content key) + 8 (AES-KW overhead)
CIPHERTEXT_LEN    = 72      // 32 (ephemeral pubkey) + 40 (wrapped key)

PBKDF2_ROUNDS     = 2048    // BIP-39 standard
PBKDF2_SALT       = b"mnemonic"  // BIP-39 standard (no passphrase)
SCHEMA_VERSION    = 1       // Embedded in HKDF info strings
```

## Key Types

| Type | Size | Rust type | Where stored |
|------|------|-----------|-------------|
| Content key | 32 bytes | `ContentKey([u8; 32])` | Wrapped in document record (never plaintext on PDS) |
| Group key | 32 bytes | `ContentKey([u8; 32])` | Wrapped per-member in keyring record. Plaintext stored locally in `keyrings/<rkey>.json` |
| X25519 public key | 32 bytes | `X25519PublicKey = [u8; 32]` | Published as `publicKey/self` record on PDS |
| X25519 private key | 32 bytes | `X25519PrivateKey = [u8; 32]` | `identity.json` (local only, 0600 permissions) |
| Ed25519 signing key | 32 bytes | `Ed25519SecretKey = [u8; 32]` | `identity.json` (local only) |
| Ed25519 verify key | 32 bytes | `Ed25519VerifyKey = [u8; 32]` | Published as `signingKey` on `publicKey/self` record |
| DPoP key | P-256 | `DpopKeyPair` (JWK) | `session.json` (per-session, not per-identity) |

## Key Hierarchy

```
Seed phrase (24 words / 256-bit entropy)
  │
  ├─ PBKDF2-HMAC-SHA512(salt="mnemonic", rounds=2048)
  │  → 512-bit master seed
  │     │
  │     ├─ HKDF-SHA256(info="opake-v1-x25519-identity")
  │     │  → X25519 private key → X25519 public key (published)
  │     │
  │     └─ HKDF-SHA256(info="opake-v1-ed25519-signing")
  │        → Ed25519 signing key → Ed25519 verify key (published)
  │
  └─ Same phrase always produces the same keys (deterministic)


Content key (per-document, random)
  │
  ├─ Direct encryption: wrapped to each authorized DID's X25519 pubkey
  │  → stored in document.encryption.envelope.keys[]
  │
  └─ Keyring encryption: wrapped under group key (AES-KW)
     → stored in document.encryption.keyringRef.wrappedContentKey

Group key (per-keyring, random)
  │
  └─ Wrapped to each member's X25519 pubkey
     → stored in keyring.members[].wrappedKey
     → plaintext stored locally in keyrings/<rkey>.json (per-rotation)
```

## Operations

### Content encryption (`content.rs`)

```
encrypt_blob(content_key, plaintext, rng)
  → AES-256-GCM(key=content_key, nonce=random_96bit, plaintext)
  → { ciphertext, nonce }

decrypt_blob(content_key, { ciphertext, nonce })
  → AES-256-GCM decrypt
  → plaintext
```

Nonce is 12 bytes, generated fresh per encryption. The PDS blob is the raw ciphertext — no framing or headers.

### Asymmetric key wrapping (`key_wrapping.rs`)

```
wrap_key(content_key, recipient_pubkey, recipient_did, rng)
  → ephemeral_secret = X25519 random
  → ephemeral_pubkey = X25519 public from ephemeral_secret
  → shared_secret = ECDH(ephemeral_secret, recipient_pubkey)
  → wrapping_key = HKDF-SHA256(
      ikm = shared_secret,
      salt = None,
      info = "opake-v{SCHEMA_VERSION}-x25519-hkdf-a256kw-{recipient_did}"
    )
  → wrapped = AES-256-KW(key=wrapping_key, plaintext=content_key)
  → ciphertext = ephemeral_pubkey ‖ wrapped   [72 bytes total]
  → WrappedKey { did, ciphertext, algo }
```

The HKDF info string includes the recipient DID for domain separation — wrapping the same content key to two different recipients produces different ciphertext even with the same ephemeral key (which doesn't happen anyway since ephemeral keys are one-time, but defense in depth).

```
unwrap_key(wrapped_key, private_key)
  → split ciphertext: ephemeral_pubkey [0..32], wrapped [32..72]
  → shared_secret = ECDH(private_key, ephemeral_pubkey)
  → wrapping_key = HKDF-SHA256(same params, using wrapped_key.did)
  → content_key = AES-256-KW unwrap(key=wrapping_key, ciphertext=wrapped)
```

### Symmetric key wrapping (`keyring_wrapping.rs`)

```
wrap_content_key_for_keyring(content_key, group_key)
  → AES-256-KW(key=group_key, plaintext=content_key)
  → 40 bytes (32 key + 8 integrity)

unwrap_content_key_from_keyring(wrapped, group_key)
  → AES-256-KW unwrap
  → content_key
```

No HKDF, no ephemeral keys — this is pure symmetric wrapping. The group key IS the KEK.

### Group key creation (`key_wrapping.rs`)

```
create_group_key(members: &[DidMember], rng)
  → group_key = random 256 bits
  → for each member:
      wrap_key(group_key, member.public_key, member.did, rng)
  → (group_key, Vec<WrappedKey>)
```

Callers pair the returned `WrappedKey`s with roles to build `KeyringMember` entries. The crypto layer doesn't know about roles.

### Metadata encryption (`metadata.rs`)

```
encrypt_metadata<T: Serialize>(key, metadata, rng)
  → json = serde_json::to_vec(metadata)
  → AES-256-GCM(key, json, random_nonce)
  → EncryptedMetadata { ciphertext: base64, nonce: base64 }

decrypt_metadata<T: DeserializeOwned>(key, encrypted)
  → decode base64 ciphertext + nonce
  → AES-256-GCM decrypt
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
  "ciphertext": { "$bytes": "<72 bytes base64>" },
  "algo": "x25519-hkdf-a256kw"
}
```

Used everywhere: document encryption envelopes, grants, pairing. No role — this is a pure crypto artifact.

### KeyringMember (domain type)

```json
{
  "wrappedKey": { "did": "...", "ciphertext": "...", "algo": "..." },
  "role": "manager"
}
```

Only used in keyring `members` and `keyHistory` arrays. Composes a `WrappedKey` with a workspace role. The role is plaintext because the AppView needs it for authorization — it's not a crypto concept.

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

## Key Rotation

On member removal, the group key rotates:

1. Archive current `members` array into `keyHistory` (minus the removed member)
2. Generate new group key
3. Wrap new group key to each remaining member (preserving their roles)
4. Increment `rotation` counter
5. Re-encrypt keyring metadata under new group key

Documents encrypted under the old group key remain readable — clients look up the document's `rotation` in `keyHistory` to find the old wrapped group key. New documents use the new group key. The removed member cannot decrypt anything created after rotation.

Old group keys are stored locally per-rotation in `keyrings/<rkey>.json`:
```json
{ "keys": [{ "rotation": 0, "group_key": "base64..." }, { "rotation": 1, "group_key": "base64..." }] }
```

## RNG Injection

All crypto functions take `&mut (impl CryptoRng + RngCore)` — no global `OsRng`. This makes the crypto layer:
- Testable (inject a deterministic RNG)
- WASM-compatible (OsRng works in browsers via `getrandom` + Web Crypto)
- Explicit about where randomness enters the system

Production callers pass `&mut OsRng`. The WASM layer does the same — `OsRng` in wasm32 delegates to `crypto.getRandomValues()`.

## Memory Safety

Sensitive types are zeroized on drop to prevent key material lingering in memory:

- **`RedactedDebug` derive macro** (opake-derive) generates three impls for structs with `#[redact]` fields:
  1. `Debug` — shows `[N bytes]` instead of content
  2. `Zeroize` — overwrites redacted fields with zeros
  3. `Drop` — calls `zeroize()` automatically

- **Types with automatic zeroization:**
  - `ContentKey` — AES-256 content encryption key
  - `Identity` — private_key and signing_key fields (base64 strings zeroed)
  - `LegacySession` — access_jwt, refresh_jwt
  - `OAuthSession` — access_token, refresh_token
  - `Cabinet` — raw X25519 private key bytes (explicit `#[derive(Zeroize, ZeroizeOnDrop)]`)
  - `Workspace` — workspace key / ContentKey (explicit `#[derive(Zeroize, ZeroizeOnDrop)]`)

- `ContentKey` is intentionally NOT `Copy` — `Copy` types can be implicitly duplicated, escaping zeroization. `Clone` requires explicit intent.

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
| Deterministic recovery | Yes | BIP-39 mnemonic → same keys every time |

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

AES-GCM-SIV is planned for `SCHEMA_VERSION` v2 as a cipher swap ([#331](https://tangled.org/sans-self.org/opake.app/issues/331)). The migration path: version-gated decrypt (v1 = AES-GCM, v2 = AES-GCM-SIV), SIV-only encrypt going forward. Existing v1 records remain readable. No key derivation changes, no identity migration — just a cipher swap with a proactive re-encryption command for users who want to upgrade old records.

## Post-Quantum

The `opakeVersion` field on all records enables a future hybrid upgrade. The plan is to add a post-quantum KEM (e.g. ML-KEM/Kyber) alongside X25519, with the version bump signaling which scheme to use. The current version (1) is X25519-only. Deferred — low practical risk for the threat model.
