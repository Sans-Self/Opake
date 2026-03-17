// TypeScript equivalents of opake-core storage types.
// Mirrors: crates/opake-core/src/storage.rs

import type { DpopKeyPair } from "./cryptoTypes";

export interface Config {
  readonly defaultDid: string | null;
  readonly accounts: Readonly<Record<string, AccountEntry>>;
  /** Whether to cache PDS records locally. Defaults to true when absent. */
  readonly cacheEnabled?: boolean;
}

// ---------------------------------------------------------------------------
// Cache types (mirrors CachedRecord / CachedCollection in storage.rs)
// ---------------------------------------------------------------------------

export interface CachedRecord<T = unknown> {
  readonly uri: string;
  readonly cid: string;
  readonly value: T;
}

export interface CachedCollection<T = unknown> {
  readonly records: readonly CachedRecord<T>[];
  readonly fetchedAt: number;
}

export interface AccountEntry {
  readonly pdsUrl: string;
  readonly handle: string;
}

export interface Identity {
  readonly did: string;
  readonly public_key: string; // base64 X25519
  readonly private_key: string; // base64 X25519
  readonly signing_key: string | null; // base64 Ed25519
  readonly verify_key: string | null; // base64 Ed25519
}

// Mirrors: opake-core Session enum (client/xrpc/mod.rs)
// Discriminated union — the `type` tag matches Rust's #[serde(tag = "type")]

export interface LegacySession {
  readonly type: "legacy";
  readonly did: string;
  readonly handle: string;
  readonly accessJwt: string;
  readonly refreshJwt: string;
}

export interface OAuthSession {
  readonly type: "oauth";
  readonly did: string;
  readonly handle: string;
  accessToken: string;
  refreshToken: string;
  readonly dpopKey: DpopKeyPair;
  readonly tokenEndpoint: string;
  dpopNonce: string | null;
  expiresAt: number | null;
  readonly clientId: string;
}

export type Session = LegacySession | OAuthSession;
