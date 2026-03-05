// TypeScript equivalents of opake-core storage types.
// Mirrors: crates/opake-core/src/storage.rs

import type { DpopKeyPair } from "./crypto-types";

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

// Mirrors: opake-core Session enum (client/xrpc/mod.rs)
// Discriminated union — the `type` tag matches Rust's #[serde(tag = "type")]

export interface LegacySession {
  type: "legacy";
  did: string;
  handle: string;
  accessJwt: string;
  refreshJwt: string;
}

export interface OAuthSession {
  type: "oauth";
  did: string;
  handle: string;
  accessToken: string;
  refreshToken: string;
  dpopKey: DpopKeyPair;
  tokenEndpoint: string;
  dpopNonce: string | null;
  expiresAt: number | null;
  clientId: string;
}

export type Session = LegacySession | OAuthSession;
