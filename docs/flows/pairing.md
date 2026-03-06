# Device Pairing

Transfer an encryption identity from an existing device to a new one, using the PDS as a relay. Both devices are authenticated to the same DID.

## Pair Request (new device)

```mermaid
sequenceDiagram
    participant User
    participant CLI as CLI (new device)
    participant Crypto
    participant PDS

    User->>CLI: opake pair request

    CLI->>CLI: Verify no local identity exists

    CLI->>Crypto: generate_ephemeral_keypair()
    Crypto-->>CLI: { public_key, private_key }

    CLI->>PDS: createRecord(pairRequest)<br/>{ ephemeralKey, algo: "x25519" }
    PDS-->>CLI: { uri, cid }

    CLI->>User: Fingerprint: a1:b2:c3:d4:e5:f6:g7:h8
    CLI->>User: Run `opake pair approve` on existing device

    loop Poll every 3s
        CLI->>PDS: listRecords(pairResponse)
        PDS-->>CLI: records[]
        CLI->>CLI: Filter by response.request == our request URI
    end

    Note over CLI: Response found — see "Receive" below
```

The ephemeral private key stays in memory. The fingerprint (first 8 bytes of the public key, hex-encoded) is displayed for out-of-band verification.

## Pair Approve (existing device)

```mermaid
sequenceDiagram
    participant User
    participant CLI as CLI (existing device)
    participant Crypto
    participant PDS

    User->>CLI: opake pair approve

    CLI->>CLI: Load local identity

    CLI->>PDS: listRecords(pairRequest)
    PDS-->>CLI: Pending requests

    CLI->>User: [1] 2026-03-06T14:00:00Z — fingerprint: a1:b2:c3:d4:...
    User-->>CLI: 1

    CLI->>Crypto: generate_content_key()
    Crypto-->>CLI: K (256-bit AES key)

    CLI->>CLI: Serialize identity → JSON bytes

    CLI->>Crypto: encrypt_blob(K, identity_json)
    Crypto-->>CLI: { ciphertext, nonce }

    CLI->>Crypto: wrap_key(K, ephemeral_pubkey, did)
    Crypto-->>CLI: wrappedKey

    CLI->>PDS: createRecord(pairResponse)<br/>{ request, wrappedKey, ciphertext, nonce }
    PDS-->>CLI: { uri, cid }

    CLI->>User: Identity sent.
```

The identity payload includes the X25519 encryption keypair, Ed25519 signing keypair, and the DID — everything needed to operate as that account.

## Receive (new device, after poll succeeds)

```mermaid
sequenceDiagram
    participant CLI as CLI (new device)
    participant Crypto
    participant PDS

    Note over CLI: Poll found a matching pairResponse

    CLI->>Crypto: unwrap_key(wrappedKey, ephemeral_private_key)
    Crypto-->>CLI: K (content key)

    CLI->>Crypto: decrypt_blob(K, ciphertext, nonce)
    Crypto-->>CLI: identity JSON bytes

    CLI->>CLI: Deserialize → Identity

    CLI->>PDS: getRecord(publicKey/self)
    PDS-->>CLI: Published public key

    CLI->>CLI: Verify identity's public key == published key

    CLI->>CLI: Save identity.json (0600)

    CLI->>PDS: deleteRecord(pairRequest)
    CLI->>PDS: deleteRecord(pairResponse)

    CLI->>CLI: Pairing complete
```

The verification step guards against a corrupted or tampered response — the derived public key must match what's already published on the PDS.

## Login Detection

When `opake login` runs on a device without a local identity, it checks for an existing `publicKey/self` record on the PDS before generating a new keypair:

```mermaid
sequenceDiagram
    participant CLI
    participant PDS

    CLI->>PDS: getRecord(publicKey/self)

    alt No published key (new user)
        CLI->>CLI: Generate identity, publish key
    else Published key exists, no local identity
        CLI->>CLI: Save session only
        CLI->>CLI: Print: "Run opake pair request"
    else Published key exists, local identity matches
        CLI->>CLI: Proceed normally
    end
```

This prevents accidental key overwrites that would break encryption on the existing device.

## Security Properties

- **Ephemeral key exchange** — the DH keypair exists only in memory during the pairing session. No long-term secret is exposed in the PDS records.
- **Visual SAS** — key fingerprints are displayed for comparison but not programmatically enforced. True zero-trust verification is a follow-up.
- **Same encryption as documents** — the identity payload uses AES-256-GCM + x25519-hkdf-a256kw, the same primitives as file encryption. No new crypto.
- **Record cleanup** — both pairing records are deleted after transfer. Stale request cleanup is tracked separately.
