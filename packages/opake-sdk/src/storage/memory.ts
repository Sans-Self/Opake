// In-memory Storage implementation for testing and non-browser environments.
//
// All data lives in Maps and is lost when the process exits. Useful for
// unit tests, scripts, and platforms without IndexedDB.

import type {
  CachedCollection,
  CachedRecord,
  Config,
  Identity,
  Session,
  Storage,
} from "../storage";
import { StorageError } from "../storage";

/**
 * In-memory storage backend.
 *
 * Data lives in Maps — nothing persists across process restarts. Intended
 * for tests and short-lived scripts.
 *
 * @example
 * ```typescript
 * import { Opake } from "@opake/sdk";
 * import { MemoryStorage } from "@opake/sdk";
 *
 * const storage = new MemoryStorage();
 * // Pre-populate for tests:
 * await storage.saveConfig({ accounts: {}, default_did: "did:plc:test" });
 * await storage.saveSession("did:plc:test", testSession);
 * await storage.saveIdentity("did:plc:test", testIdentity);
 *
 * const opake = await Opake.init({ storage });
 * ```
 */
export class MemoryStorage implements Storage {
  private config: Config = { accounts: {} };
  private readonly identities = new Map<string, Identity>();
  private readonly sessions = new Map<string, Session>();
  private readonly pairStates = new Map<string, Uint8Array>();
  private readonly cacheRecords = new Map<string, Map<string, CachedRecord>>();
  private readonly cacheMeta = new Map<string, number>();

  async loadConfig(): Promise<Config> {
    return this.config;
  }

  async saveConfig(config: Config): Promise<void> {
    this.config = config;
  }

  async loadIdentity(did: string): Promise<Identity> {
    const identity = this.identities.get(did);
    if (!identity) throw new StorageError(`No identity for ${did}`);
    return identity;
  }

  async saveIdentity(did: string, identity: Identity): Promise<void> {
    this.identities.set(did, identity);
  }

  async loadSession(did: string): Promise<Session> {
    const session = this.sessions.get(did);
    if (!session) throw new StorageError(`No session for ${did}`);
    return session;
  }

  async saveSession(did: string, session: Session): Promise<void> {
    this.sessions.set(did, session);
  }

  async clearSession(did: string): Promise<void> {
    this.sessions.delete(did);
  }

  // -- Pair state ------------------------------------------------------------

  private pairStateKey(did: string, rkey: string): string {
    return `${did}::${rkey}`;
  }

  async savePairState(did: string, rkey: string, privateKey: Uint8Array): Promise<void> {
    // Copy the buffer so later mutations by the caller don't affect stored state.
    this.pairStates.set(this.pairStateKey(did, rkey), new Uint8Array(privateKey));
  }

  async loadPairState(did: string, rkey: string): Promise<Uint8Array> {
    const key = this.pairStateKey(did, rkey);
    const bytes = this.pairStates.get(key);
    if (!bytes) throw new StorageError(`No pair state for ${did}/${rkey}`);
    return new Uint8Array(bytes);
  }

  async deletePairState(did: string, rkey: string): Promise<void> {
    this.pairStates.delete(this.pairStateKey(did, rkey));
  }

  async removeAccount(did: string): Promise<void> {
    const remainingAccounts = Object.fromEntries(
      Object.entries(this.config.accounts).filter(([key]) => key !== did),
    );
    const remaining = Object.keys(remainingAccounts);
    this.config = {
      ...this.config,
      accounts: remainingAccounts,
      default_did:
        this.config.default_did === did
          ? remaining.length > 0
            ? remaining[0]
            : undefined
          : this.config.default_did,
    };
    this.identities.delete(did);
    this.sessions.delete(did);
    const pairPrefix = `${did}::`;
    for (const key of [...this.pairStates.keys()]) {
      if (key.startsWith(pairPrefix)) this.pairStates.delete(key);
    }
    await this.cacheClear(did);
  }

  // -- Cache: record-level ---------------------------------------------------

  private cacheKey(did: string, collection: string): string {
    return `${did}::${collection}`;
  }

  async cacheGetRecord<T>(
    did: string,
    collection: string,
    uri: string,
  ): Promise<CachedRecord<T> | null> {
    const records = this.cacheRecords.get(this.cacheKey(did, collection));
    return (records?.get(uri) as CachedRecord<T> | undefined) ?? null;
  }

  async cachePutRecords<T>(
    did: string,
    collection: string,
    records: readonly CachedRecord<T>[],
  ): Promise<void> {
    const key = this.cacheKey(did, collection);
    let map = this.cacheRecords.get(key);
    if (!map) {
      map = new Map();
      this.cacheRecords.set(key, map);
    }
    for (const record of records) {
      map.set(record.uri, record as CachedRecord);
    }
  }

  async cacheRemoveRecord(did: string, collection: string, uri: string): Promise<void> {
    this.cacheRecords.get(this.cacheKey(did, collection))?.delete(uri);
  }

  // -- Cache: collection-level -----------------------------------------------

  async cacheGetCollection<T>(
    did: string,
    collection: string,
  ): Promise<CachedCollection<T> | null> {
    const key = this.cacheKey(did, collection);
    const fetchedAt = this.cacheMeta.get(key);
    if (fetchedAt === undefined) return null;

    const map = this.cacheRecords.get(key);
    const records = map ? [...map.values()] : [];
    return { records: records as CachedRecord<T>[], fetched_at: fetchedAt };
  }

  async cachePutCollection<T>(
    did: string,
    collection: string,
    data: CachedCollection<T>,
  ): Promise<void> {
    const key = this.cacheKey(did, collection);
    const map = new Map<string, CachedRecord>();
    for (const record of data.records) {
      map.set(record.uri, record as CachedRecord);
    }
    this.cacheRecords.set(key, map);
    this.cacheMeta.set(key, data.fetched_at);
  }

  async cacheInvalidateCollection(did: string, collection: string): Promise<void> {
    this.cacheMeta.delete(this.cacheKey(did, collection));
  }

  // -- Cache: account-level --------------------------------------------------

  async cacheClear(did: string): Promise<void> {
    const prefix = `${did}::`;
    for (const key of [...this.cacheRecords.keys()]) {
      if (key.startsWith(prefix)) this.cacheRecords.delete(key);
    }
    for (const key of [...this.cacheMeta.keys()]) {
      if (key.startsWith(prefix)) this.cacheMeta.delete(key);
    }
  }
}
