// TypeScript equivalents of opake-core storage types.
// Mirrors: crates/opake-core/src/storage.rs

export interface Config {
  defaultDid: string | null;
  accounts: Record<string, AccountConfig>;
  appviewUrl: string | null;
}

export interface AccountConfig {
  pdsUrl: string;
  handle: string;
}

export interface Identity {
  did: string;
  publicKey: string; // base64 X25519
  privateKey: string; // base64 X25519
  signingKey: string | null; // base64 Ed25519
  verifyKey: string | null; // base64 Ed25519
}

export interface Session {
  did: string;
  handle: string;
  accessJwt: string;
  refreshJwt: string;
}
