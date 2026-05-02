# Encryption Primitives

## Key Wrapping (x25519-mlkem768-hkdf-a256kw-v2)

How a symmetric content key gets wrapped to a recipient's hybrid public key (X25519 + ML-KEM-768). Defends against harvest-now-decrypt-later per BSI TR-02102 / ANSSI guidance. Both KEM outputs are combined via HKDF before use as a KEK.

```mermaid
flowchart LR
    subgraph Wrap ["wrap_key()"]
        direction TB
        EphX["Ephemeral X25519<br/>keypair"] --> ECDH
        RecipX["Recipient<br/>X25519 pubkey"] --> ECDH
        ECDH["X25519 shared secret"] --> HKDF

        EphML["Ephemeral ML-KEM-768<br/>encapsulation"] --> MLSec
        RecipML["Recipient<br/>ML-KEM-768 pubkey"] --> EphML
        MLSec["ML-KEM-768 shared secret"] --> HKDF

        HKDF["HKDF-SHA256<br/>info = 'opake-v1-x25519-mlkem768-hkdf-a256kw-v2-{did}'"] --> KEK
        KEK["256-bit key<br/>encryption key"] --> AESKW
        ContentKey["Content key K<br/>(AES-256)"] --> AESKW
        AESKW["AES-256-KW"] --> Ciphertext
    end

    Ciphertext["wrappedKey.ciphertext:<br/>[32B X25519 pubkey ‖ 1088B ML-KEM ct ‖ 40B wrapped key]"]

    style Wrap fill:#1a1a2e,color:#eee
    style Ciphertext fill:#16213e,color:#eee
```

## Keyring Key Wrapping (AES-256-KW)

How a content key gets wrapped under a keyring's group key. Symmetric wrap — no ECDH, no ephemeral keys.

```mermaid
flowchart LR
    subgraph Wrap ["wrap_content_key_for_keyring()"]
        direction TB
        GK["Group key GK<br/>(AES-256)"] --> KEK["AES-256-KW<br/>(RFC 3394)"]
        ContentKey["Content key K<br/>(AES-256)"] --> KEK
    end

    KEK --> Wrapped["40 bytes<br/>(32B key + 8B integrity)"]

    style Wrap fill:#1a1a2e,color:#eee
    style Wrapped fill:#16213e,color:#eee
```

The group key itself is wrapped to each member's X25519 public key using the asymmetric wrapping scheme above.

## Content Encryption (AES-256-GCM)

```mermaid
flowchart LR
    subgraph Encrypt ["encrypt_blob()"]
        direction TB
        K["Content key K"] --> GCM
        Nonce["Random 12-byte nonce"] --> GCM
        Plaintext["File bytes"] --> GCM
        GCM["AES-256-GCM"]
    end

    GCM --> Ciphertext["Ciphertext + auth tag"]
    GCM --> StoredNonce["Nonce stored in<br/>document record"]

    style Encrypt fill:#1a1a2e,color:#eee
```
