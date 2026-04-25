// Mermaid sources for the /build/ docs pages.
//
// Kept in a `.ts` file rather than inline in the `.mdx` files because MDX 3
// dedents template literals inside `.mdx`, which mangles mermaid whitespace.

/** Device pairing via the CLI. Shows which command runs on which device. */
export const pairingCliFlow = `sequenceDiagram
    participant New as New device
    participant PDS as Your PDS
    participant Old as Existing device

    Note over New: opake pair request
    New->>New: generate one-time keypair
    New->>PDS: publish pair request<br/>(new device's public half)

    Note over Old: opake pair approve
    Old->>PDS: poll for pair requests
    PDS-->>Old: request record
    Old->>Old: wrap identity keys<br/>with new device's public half
    Old->>PDS: publish pair response

    New->>PDS: poll for pair response
    PDS-->>New: response record
    New->>New: unwrap with one-time key<br/>→ identity installed
    New->>PDS: delete both records
`;
