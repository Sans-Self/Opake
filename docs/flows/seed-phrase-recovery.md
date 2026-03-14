# Seed Phrase Recovery

Deterministic keypair derivation from a BIP-39 mnemonic (24 words / 256-bit entropy). The seed phrase is the last-resort recovery mechanism when you've lost access to all devices. Same phrase always produces the same X25519 encryption keypair and Ed25519 signing keypair.

For setting up additional devices when you still have access to an existing one, use [device pairing](pairing.md) instead — it's faster and doesn't require typing 24 words.

## Derivation Pipeline

```mermaid
flowchart LR
    subgraph Generate ["First-time setup (login on fresh account)"]
        direction LR
        Entropy["256 bits of entropy<br/>(from OS CSPRNG)"] --> Mnemonic
        Mnemonic["BIP-39 mnemonic<br/>(24 words)"]
    end

    subgraph Derive ["Keypair derivation (every login / recovery)"]
        direction LR
        Mnemonic --> PBKDF["BIP-39 seed derivation<br/>PBKDF2-HMAC-SHA512<br/>2048 rounds, salt = 'mnemonic'"]
        PBKDF --> Seed["512-bit master seed"]
        Seed --> HKDFX["HKDF-SHA256<br/>info = 'opake-v1-x25519-identity'"]
        Seed --> HKDFE["HKDF-SHA256<br/>info = 'opake-v1-ed25519-signing'"]
        HKDFX --> X25519["32-byte X25519<br/>private key → public key"]
        HKDFE --> Ed25519["32-byte Ed25519<br/>signing key → verify key"]
    end

    style Generate fill:#1a1a2e,color:#eee
    style Derive fill:#16213e,color:#eee
```

The mnemonic is the root secret — both keypairs are derived from it. Losing the phrase means losing access to all encrypted data and signing capability. The HKDF info strings include the schema version for domain separation, consistent with the key wrapping convention.

## First Login (New Account)

Seed phrase generation is the default identity creation path. No random keypair fallback.

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Crypto
    participant PDS

    User->>CLI: opake login <handle>
    CLI->>PDS: OAuth login (DPoP)
    PDS-->>CLI: session tokens

    CLI->>Crypto: Generate 256-bit entropy (CSPRNG)
    Crypto-->>CLI: 24-word mnemonic

    CLI->>User: Display seed phrase in 4×6 numbered grid
    CLI->>User: Offer to save as .txt file (0600 permissions)
    CLI->>User: Confirm 3 random words

    CLI->>Crypto: mnemonic → PBKDF2 → HKDF (x2) → X25519 + Ed25519
    Crypto-->>CLI: identity (both keypairs)

    CLI->>CLI: Save identity.json
    CLI->>PDS: putRecord (publicKey/self)
    PDS-->>CLI: { uri, cid }

    CLI->>User: Logged in as <handle>
```

The web UI follows the same flow via WASM: generate → display grid → copy/download → confirm 3 words → derive → publish.

## Recovery (Existing Account, New Device)

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Crypto
    participant PDS

    User->>CLI: opake recover
    CLI->>User: Enter 24-word seed phrase

    CLI->>Crypto: parse_mnemonic (validate checksum)
    Crypto-->>CLI: valid

    CLI->>Crypto: mnemonic → PBKDF2 → HKDF (x2) → X25519 + Ed25519
    Crypto-->>CLI: identity

    CLI->>PDS: getRecord (publicKey/self)
    PDS-->>CLI: published public key

    alt Keys match
        CLI->>CLI: Save identity.json
        CLI->>PDS: putRecord (publicKey/self)
        CLI->>User: Identity recovered
    else Keys don't match
        CLI->>User: WARNING: derived key ≠ published key
        CLI->>User: Confirm 'save anyway' or cancel
    end
```

The CLI also supports `opake recover --file seed.txt` for importing from a .txt backup. The file parser is lenient — handles numbered grids, numbered lists, plain word lists.

## Key Mismatch

A mismatch between the derived public key and the published `publicKey/self` means:

- Wrong seed phrase (for a different account)
- Account was set up before seed phrases were the default (random keypair)

If the user confirms, the new key is published but existing data encrypted to the old key remains unreadable. The CLI and web UI both warn about this explicitly.

## Grid Format

The seed phrase is displayed and stored as a 4-column × 6-row numbered grid:

```
 1. mimic          7. action         13. zebra          19. crawl
 2. short          8. bundle         14. rain           20. pen
 3. have           9. wagon          15. addict         21. flavor
 4. picnic        10. wrong          16. legend         22. useful
 5. neck          11. balance        17. swallow        23. caution
 6. awful         12. palm           18. space          24. focus
```

This format is shared between CLI (`format_mnemonic_grid` / `parse_mnemonic_grid` in opake-core) and the web UI. The parser handles column-major reordering when numbers are present.
