export interface AtBytes {
  $bytes: string
}

export interface WrappedKey {
  did: string
  ciphertext: AtBytes
  algo: string
}

export interface EncryptedPayload {
  ciphertext: Uint8Array
  nonce: Uint8Array
}

// Mirrors: opake-core DpopPublicJwk (client/dpop.rs)
export interface DpopPublicJwk {
  kty: string
  crv: string
  x: string
  y: string
}

// Mirrors: opake-core DpopKeyPair (client/dpop.rs)
// Serialized via serde — field names match Rust's #[serde(rename)]
export interface DpopKeyPair {
  privateKey: string // base64url P-256 secret
  publicJwk: DpopPublicJwk
}

// Mirrors: opake-core PkceChallenge (client/oauth_discovery.rs)
export interface PkceChallenge {
  verifier: string
  challenge: string
}

// Mirrors: opake-core EphemeralKeypair (crypto/mod.rs)
export interface EphemeralKeypair {
  publicKey: Uint8Array
  privateKey: Uint8Array
}
