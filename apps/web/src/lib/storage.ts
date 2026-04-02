// Platform-agnostic storage contract.
// Mirrors: crates/opake-core/src/storage.rs — Storage trait

import type { CachedCollection, CachedRecord, Config, Identity, Session } from "./storageTypes";

export interface Storage {
  loadConfig(): Promise<Config>;
  saveConfig(config: Config): Promise<void>;
  loadIdentity(did: string): Promise<Identity>;
  saveIdentity(did: string, identity: Identity): Promise<void>;
  loadSession(did: string): Promise<Session>;
  saveSession(did: string, session: Session): Promise<void>;
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

  /** All cached records + fetched_at, or null if never fully fetched. */
  cacheGetCollection<T>(did: string, collection: string): Promise<CachedCollection<T> | null>;
  /** Atomically replace all records for a collection and set fetched_at. */
  cachePutCollection<T>(did: string, collection: string, data: CachedCollection<T>): Promise<void>;
  /** Clear fetched_at (records stay for offline / record-level use). */
  cacheInvalidateCollection(did: string, collection: string): Promise<void>;

  // -- Cache: account-level --------------------------------------------------

  /** Remove all cached data for an account. */
  cacheClear(did: string): Promise<void>;
}

export class StorageError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "StorageError";
  }
}

/** `did:plc:abc` → `did_plc_abc` — mirrors `sanitize_did` in opake-core. */
export function sanitizeDid(did: string): string {
  return did.replaceAll(":", "_");
}
