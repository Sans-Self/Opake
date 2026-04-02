export interface AtBytes {
  $bytes: string;
}

export interface WrappedKey {
  did: string;
  ciphertext: AtBytes;
  algo: string;
}

export interface EncryptedPayload {
  ciphertext: Uint8Array;
  nonce: Uint8Array;
}

// Mirrors: opake-core DpopPublicJwk (client/dpop.rs)
export interface DpopPublicJwk {
  kty: string;
  crv: string;
  x: string;
  y: string;
}

// Mirrors: opake-core DpopKeyPair (client/dpop.rs)
export interface DpopKeyPair {
  private_key_b64: string; // base64url P-256 secret
  public_jwk: DpopPublicJwk;
}

// Mirrors: opake-core PkceChallenge (client/oauth_discovery.rs)
export interface PkceChallenge {
  verifier: string;
  challenge: string;
}

// Mirrors: opake-core EphemeralKeypair (crypto/mod.rs)
export interface EphemeralKeypair {
  public_key: Uint8Array;
  private_key: Uint8Array;
}
