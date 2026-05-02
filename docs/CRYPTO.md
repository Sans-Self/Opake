# Opake — Cryptography Reference

Quick-reference for every algorithm, constant, and key type in the system. For the conceptual overview (why these choices, what the tradeoffs are), see [ARCHITECTURE.md](ARCHITECTURE.md).

## Algorithms

| Name | Use | Library |
|------|-----|---------|
| AES-256-GCM | Content encryption (blobs + metadata) | `aes-gcm` |
| X25519 + ML-KEM-768 (HKDF-A256KW) | Hybrid asymmetric key wrapping | `x25519-dalek` + `libcrux-ml-kem` + `hkdf` + `aes-kw` |
| AES-256-KW (RFC 3394) | Symmetric key wrapping (content key → group key) | `aes-kw` |
| HKDF-SHA256 | KDF for key wrapping + identity derivation | `hkdf` + `sha2` |
| PBKDF2-HMAC-SHA512 | Mnemonic → master seed | `pbkdf2` + `sha2` |
| Ed25519 | Indexer authentication signatures | `ed25519-dalek` |
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
```

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
       expand_info  = "opake-v{SCHEMA_VERSION}-x25519-mlkem768-hkdf-a256kw-v2-{recipient_did}",
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
  "ciphertext": { "$bytes": "<1160 bytes base64>" },
  "algo": "x25519-mlkem768-hkdf-a256kw-v2"
}
```

Used everywhere: document encryption envelopes, grants, group keyring members, pair-flow responses. The 1160-byte ciphertext layout is `eph_x25519_pub (32) ‖ ml_kem_ct (1088) ‖ aes_kw_wrapped (40)`.

### KeyringMember (domain type)

```json
{
  "wrappedKey": { "did": "...", "ciphertext": "...", "algo": "..." },
  "role": "manager"
}
```

Only used in keyring `members` and `keyHistory` arrays. Composes a `WrappedKey` with a workspace role. The role is plaintext because the Indexer needs it for authorization — it's not a crypto concept.

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

Pairing a fresh device to an existing identity uses the same hybrid construction as everywhere else. The new device generates an ephemeral hybrid keypair (X25519 + ML-KEM-768), publishes both public halves in `app.opake.pairRequest`, and the responding device wraps the existing identity to that bundle via `wrap_key`. The `pairResponse` record carries a single 1160-byte `WrappedKey` and an AES-256-GCM ciphertext of the serialized `Identity`.

The new device's ephemeral private bundle is persisted to local Storage (32 + 2400 = 2432 bytes, X25519 ‖ ML-KEM-768) under `(did, request_rkey)` so it survives between request and response — the response can take minutes to days to arrive.

## Key Rotation

On member removal, the group key rotates:

1. Archive current `members` array into `keyHistory` (minus the removed member)
2. Generate new group key
3. Hybrid-wrap the new group key to each remaining member's `PublicKeyBundle` (preserving their roles)
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
