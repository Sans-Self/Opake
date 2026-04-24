// Mermaid sources for the /understand/ docs pages.
//
// Kept in a `.ts` file rather than inline in the `.mdx` files because MDX 3
// dedents template literals inside `.mdx`, which mangles mermaid whitespace
// (mermaid is sensitive to leading indentation on lines inside
// sequenceDiagram / flowchart blocks). Imports bypass that.
//
// These are simplified versions of the internal flows in `docs/flows/*.md` —
// implementation-detail names like `FileManager`, `#[signoff]`, and
// `com.atproto.repo.createRecord` are collapsed to the underlying operation
// a reader actually needs to picture.

/** Encrypt-and-upload, end-to-end. */
export const uploadFlow = `sequenceDiagram
    participant App as Opake App
    participant Crypto as Client-side crypto
    participant PDS as Your PDS

    App->>Crypto: generate random content key K (256-bit)
    App->>Crypto: encrypt file with K
    Crypto-->>App: ciphertext blob

    App->>PDS: upload ciphertext as a blob
    PDS-->>App: blob reference (CID)

    App->>Crypto: wrap K with your public key
    Crypto-->>App: wrappedKey

    App->>Crypto: encrypt metadata with K
    Crypto-->>App: encrypted metadata

    App->>PDS: publish document record<br/>(blob ref + wrappedKey + encrypted metadata)
    PDS-->>App: record URI

    Note over PDS: PDS never sees<br/>the plaintext or K
`;

/** Granting access to another user and the recipient's download. */
export const sharingFlow = `sequenceDiagram
    participant You
    participant YourPDS as Your PDS
    participant Indexer
    participant Friend

    You->>YourPDS: resolve friend's handle → DID
    You->>YourPDS: fetch friend's public encryption key

    You->>You: wrap file's content key<br/>with friend's public key
    You->>YourPDS: publish Grant record<br/>(document URI + wrappedKey)

    YourPDS-->>Indexer: firehose event: new grant
    Indexer-->>Friend: inbox: "new share from you"

    Friend->>YourPDS: fetch the grant
    YourPDS-->>Friend: grant record
    Friend->>Friend: unwrap content key<br/>with their private key

    Friend->>YourPDS: fetch ciphertext blob
    YourPDS-->>Friend: ciphertext
    Friend->>Friend: decrypt with content key

    Note over YourPDS: File never leaves your PDS —<br/>friend streams it directly
`;

/** Device pairing — how identity keys reach a new device. */
export const pairingFlow = `sequenceDiagram
    participant NewDevice as New Device
    participant PDS as Your PDS
    participant OldDevice as Existing Device

    NewDevice->>NewDevice: generate one-time keypair
    NewDevice->>PDS: publish pair request<br/>(contains new device's public half)

    OldDevice->>PDS: poll for pair requests
    PDS-->>OldDevice: pair request record

    OldDevice->>OldDevice: wrap your identity<br/>with new device's public half

    OldDevice->>PDS: publish pair response<br/>(wrapped identity)

    NewDevice->>PDS: poll for pair response
    PDS-->>NewDevice: pair response record

    NewDevice->>NewDevice: unwrap with<br/>one-time private half<br/>→ identity ready

    NewDevice->>PDS: delete both records
`;

/** Turning 24 words into the two identity keypairs. */
export const derivationFlow = `flowchart LR
    A["24 words<br/><small>BIP-39 wordlist</small>"] -->|"PBKDF2-HMAC-SHA512<br/><small>2048 rounds, salt 'mnemonic'</small>"| B["512-bit<br/>master seed"]
    B -->|"HKDF-SHA256<br/><small>info: opake-v1-x25519-identity</small>"| C["X25519 keypair<br/><small>encryption + key wrapping</small>"]
    B -->|"HKDF-SHA256<br/><small>info: opake-v1-ed25519-signing</small>"| D["Ed25519 keypair<br/><small>indexer auth + record signing</small>"]

    style A fill:#F5E9D0,stroke:#9A7840
    style C fill:#EEF2E8,stroke:#5C7A54
    style D fill:#EEF2E8,stroke:#5C7A54
`;
