# Multi-Device Identity (Planned)

Deterministic keypair derivation from a BIP-39 mnemonic. Same seed on any device produces the same X25519 keypair — no key sync protocol needed. Replaces the current plaintext keypair file at `~/.config/opake/accounts/<did>/identity.json`.

## Keypair Derivation

```mermaid
flowchart TB
    subgraph Generate ["First-time setup (opake init or first login)"]
        direction TB
        Entropy["128 bits of entropy<br/>(from OS CSPRNG)"] --> Mnemonic
        Mnemonic["BIP-39 mnemonic<br/>(12 words)"]
    end

    subgraph Derive ["Keypair derivation (every login)"]
        direction TB
        Mnemonic --> PBKDF["BIP-39 seed derivation<br/>PBKDF2-HMAC-SHA512<br/>2048 rounds, salt = 'mnemonic'"]
        PBKDF --> Seed["512-bit master seed"]
        Seed --> HKDF["HKDF-SHA256<br/>info = 'opake-v1-x25519-identity'"]
        HKDF --> PrivKey["32-byte X25519<br/>private key"]
        PrivKey --> PubKey["X25519 public key<br/>(clamped, published to PDS)"]
    end

    style Generate fill:#1a1a2e,color:#eee
    style Derive fill:#16213e,color:#eee
```

The mnemonic is the root secret — everything else is derived. Losing it means losing access to all encrypted data. The HKDF info string includes the schema version for domain separation, same convention as key wrapping.

## First Login (New Device)

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Crypto
    participant PDS

    User->>CLI: opake login <handle>
    CLI->>User: No identity found. Enter seed phrase or generate new?
    User-->>CLI: "abandon ability able about above absent ..."

    CLI->>Crypto: BIP-39 validate (checksum, wordlist)
    Crypto-->>CLI: valid

    CLI->>Crypto: mnemonic → PBKDF2 → 512-bit seed
    CLI->>Crypto: seed → HKDF-SHA256 → X25519 private key
    Crypto-->>CLI: keypair (private + public)

    CLI->>CLI: Save identity (private key to disk)

    CLI->>PDS: com.atproto.server.createSession
    PDS-->>CLI: { did, handle, accessJwt, refreshJwt }

    CLI->>PDS: putRecord (publicKey/self)
    PDS-->>CLI: { uri, cid }

    CLI->>User: Logged in as <handle>
```

If the published public key doesn't match the derived one, the CLI warns — either a wrong seed phrase or a key was published from a different seed. The user decides whether to overwrite.

## Generate New Identity

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant Crypto

    User->>CLI: opake init

    CLI->>Crypto: Generate 128 bits entropy (CSPRNG)
    Crypto-->>CLI: entropy
    CLI->>Crypto: BIP-39 encode (entropy → 12 words)
    Crypto-->>CLI: mnemonic

    CLI->>User: Your seed phrase (write this down):<br/>"abandon ability able about ..."

    CLI->>User: Confirm by entering words 3, 7, 11
    User-->>CLI: "able", "absent", "above"
    CLI->>CLI: Verify matches

    CLI->>Crypto: Derive keypair from mnemonic
    Crypto-->>CLI: keypair

    CLI->>CLI: Save identity to disk
    CLI->>User: Identity created. Run `opake login` to connect to a PDS.
```

The confirmation step guards against clipboard-and-forget. The seed phrase is shown exactly once — the CLI never stores or displays it again.

## Key Mismatch Recovery

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant PDS

    User->>CLI: opake login <handle>
    CLI->>CLI: Derive keypair from seed phrase

    CLI->>PDS: getRecord (publicKey/self)
    PDS-->>CLI: Published public key ≠ derived public key

    CLI->>User: Warning: PDS has a different public key.<br/>This means either:<br/>1. Wrong seed phrase<br/>2. Key was published from another seed

    CLI->>User: Overwrite published key? [y/N]
    User-->>CLI: y

    CLI->>PDS: putRecord (publicKey/self) with derived key
    PDS-->>CLI: { uri, cid }

    CLI->>User: Public key updated. Previous grants may be unreadable.
```

Overwriting the published key breaks any existing grants or keyring memberships that wrapped keys to the old public key. The CLI should be loud about this.
