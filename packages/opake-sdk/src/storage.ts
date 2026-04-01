// Platform-agnostic storage contract.
//
// Mirrors the Rust Storage trait in opake-core. Consumers provide an
// implementation for their platform (IndexedDB for browsers, filesystem
// for Node.js, etc). The SDK ships IndexedDbStorage as a built-in default.

// ---------------------------------------------------------------------------
// Core types (mirrors crates/opake-core/src/storage.rs)
// ---------------------------------------------------------------------------

/** Global app configuration — default account + registered accounts. */
export interface Config {
  readonly default_did?: string;
  readonly accounts: Readonly<Record<string, AccountEntry>>;
  readonly cache_enabled?: boolean;
}

export interface AccountEntry {
  readonly pds_url: string;
  readonly handle: string;
}

/** X25519 + Ed25519 keypair derived from seed phrase or generated fresh. */
export interface Identity {
  readonly did: string;
  readonly public_key: string;
  readonly private_key: string;
  readonly signing_key?: string;
  readonly verify_key?: string;
}

/** DPoP public key in JWK format (P-256). */
export interface DpopPublicJwk {
  readonly kty: string;
  readonly crv: string;
  readonly x: string;
  readonly y: string;
}

/** DPoP keypair for OAuth token binding. */
export interface DpopKeyPair {
  readonly private_key_b64: string;
  readonly public_jwk: DpopPublicJwk;
}

/** Legacy session (app passwords, pre-OAuth). */
export interface LegacySession {
  readonly type: "legacy";
  readonly did: string;
  readonly handle: string;
  readonly access_jwt: string;
  readonly refresh_jwt: string;
}

/** OAuth 2.0 session with DPoP token binding. */
export interface OAuthSession {
  readonly type: "oauth";
  readonly did: string;
  readonly handle: string;
  // Mutable — WASM writes back refreshed tokens via Storage.saveSession()
  access_token: string;
  refresh_token: string;
  readonly dpop_key: DpopKeyPair;
  readonly token_endpoint: string;
  dpop_nonce?: string;
  expires_at?: number;
  readonly client_id: string;
}

/** Discriminated union — matches the Rust `Session` enum. */
export type Session = LegacySession | OAuthSession;

// ---------------------------------------------------------------------------
// Cache types
// ---------------------------------------------------------------------------

export interface CachedRecord<T = unknown> {
  readonly uri: string;
  readonly cid: string;
  readonly value: T;
}

export interface CachedCollection<T = unknown> {
  readonly records: readonly CachedRecord<T>[];
  readonly fetched_at: number;
}

// ---------------------------------------------------------------------------
// Storage interface
// ---------------------------------------------------------------------------

/**
 * Persistent storage for Opake account data and local cache.
 *
 * Implement this interface to run Opake on your platform. The SDK ships
 * `IndexedDbStorage` for browsers and `MemoryStorage` for testing.
 *
 * All methods are async — storage may be backed by IndexedDB, filesystem,
 * or a remote service.
 *
 * @example
 * ```typescript
 * import { Opake } from "@opake/sdk";
 * import { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";
 *
 * const opake = await Opake.init({ storage: new IndexedDbStorage() });
 * ```
 */
export interface Storage {
  /** Load the global config (default account, registered accounts). */
  loadConfig(): Promise<Config>;
  /** Persist the global config. */
  saveConfig(config: Config): Promise<void>;

  /** Load the encryption identity for a DID. */
  loadIdentity(did: string): Promise<Identity>;
  /** Persist the encryption identity for a DID. */
  saveIdentity(did: string, identity: Identity): Promise<void>;

  /** Load the authentication session for a DID. */
  loadSession(did: string): Promise<Session>;
  /** Persist the authentication session for a DID. */
  saveSession(did: string, session: Session): Promise<void>;

  /** Remove all data for an account (identity, session, cache). */
  removeAccount(did: string): Promise<void>;

  // -- Cache: record-level ---------------------------------------------------

  /** Look up a single cached record by URI. */
  cacheGetRecord<T>(did: string, collection: string, uri: string): Promise<CachedRecord<T> | null>;
  /** Upsert one or more records (does not touch collection metadata). */
  cachePutRecords<T>(
    did: string,
    collection: string,
    records: readonly CachedRecord<T>[],
  ): Promise<void>;
  /** Remove a single record from the cache. */
  cacheRemoveRecord(did: string, collection: string, uri: string): Promise<void>;

  // -- Cache: collection-level -----------------------------------------------

  /** All cached records + fetched_at timestamp, or null if never fetched. */
  cacheGetCollection<T>(did: string, collection: string): Promise<CachedCollection<T> | null>;
  /** Atomically replace all records for a collection and set fetched_at. */
  cachePutCollection<T>(did: string, collection: string, data: CachedCollection<T>): Promise<void>;
  /** Clear fetched_at (records stay for offline use). Forces re-sync on next load. */
  cacheInvalidateCollection(did: string, collection: string): Promise<void>;

  // -- Cache: account-level --------------------------------------------------

  /** Remove all cached data for an account. */
  cacheClear(did: string): Promise<void>;
}

/** Thrown when a Storage operation fails. */
export class StorageError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "StorageError";
  }
}

/** Sanitize a DID for use as a storage key: `did:plc:abc` → `did_plc_abc`. */
export function sanitizeDid(did: string): string {
  return did.replaceAll(":", "_");
}
