// IndexedDB-backed Storage implementation using Dexie.js.
//
// Default storage for browsers. Published as a separate entrypoint
// (`@opake/sdk/storage/indexeddb`) so the Dexie dependency only loads
// when actually used.

import Dexie, { type EntityTable, type Table } from "dexie";
import type {
  CachedCollection,
  CachedRecord,
  Config,
  Identity,
  Session,
  Storage,
} from "../storage";
import { StorageError, sanitizeDid } from "../storage";

const CONFIG_KEY = "global";

// ---------------------------------------------------------------------------
// Row types (Dexie table shapes)
// ---------------------------------------------------------------------------

interface ConfigRow {
  key: string;
  value: Config;
}

interface IdentityRow {
  did: string;
  value: Identity;
}

interface SessionRow {
  did: string;
  value: Session;
}

interface CacheRecordRow {
  did: string;
  collection: string;
  uri: string;
  cid: string;
  value: unknown;
}

interface CacheMetaRow {
  did: string;
  collection: string;
  fetchedAt: number;
}

interface PairStateRow {
  did: string;
  rkey: string;
  privateKey: Uint8Array;
}

// ---------------------------------------------------------------------------
// Database
// ---------------------------------------------------------------------------

class OpakeDatabase extends Dexie {
  readonly configs!: Readonly<EntityTable<ConfigRow, "key">>;
  readonly identities!: Readonly<EntityTable<IdentityRow, "did">>;
  readonly sessions!: Readonly<EntityTable<SessionRow, "did">>;
  readonly pairStates!: Readonly<Table<PairStateRow>>;
  readonly cacheRecords!: Readonly<Table<CacheRecordRow>>;
  readonly cacheMeta!: Readonly<Table<CacheMetaRow>>;

  constructor(name = "opake") {
    super(name);
    this.version(1).stores({
      configs: "key",
      identities: "did",
      sessions: "did",
      cacheRecords: "[did+collection+uri], [did+collection], did",
      cacheMeta: "[did+collection], did",
    });
    // v2 adds the pair_states table for WASM-owned ephemeral pair keys.
    // Existing databases upgrade in-place; no data migration required.
    this.version(2).stores({
      configs: "key",
      identities: "did",
      sessions: "did",
      pairStates: "[did+rkey], did",
      cacheRecords: "[did+collection+uri], [did+collection], did",
      cacheMeta: "[did+collection], did",
    });
  }
}

// ---------------------------------------------------------------------------
// IndexedDbStorage
// ---------------------------------------------------------------------------

/**
 * IndexedDB storage backend using Dexie.js.
 *
 * This is the default storage for browser environments. It persists
 * config, identity, sessions, and cache across page reloads.
 *
 * Requires `dexie` as a peer dependency.
 *
 * @example
 * ```typescript
 * import { Opake } from "@opake/sdk";
 * import { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";
 *
 * const opake = await Opake.init({ storage: new IndexedDbStorage() });
 * ```
 */
export class IndexedDbStorage implements Storage {
  private readonly db: Readonly<OpakeDatabase>;

  constructor(dbName = "opake") {
    this.db = new OpakeDatabase(dbName);
  }

  // -- Config / Identity / Session ------------------------------------------

  async loadConfig(): Promise<Config> {
    const row = await this.db.configs.get(CONFIG_KEY);
    if (!row) throw new StorageError("no config found — log in first");
    return row.value;
  }

  async saveConfig(config: Config): Promise<void> {
    await this.db.configs.put({ key: CONFIG_KEY, value: config });
  }

  async loadIdentity(did: string): Promise<Identity> {
    const key = sanitizeDid(did);
    const row = await this.db.identities.get(key);
    if (!row) throw new StorageError(`no identity for ${did}`);
    return row.value;
  }

  async saveIdentity(did: string, identity: Identity): Promise<void> {
    const key = sanitizeDid(did);
    await this.db.identities.put({ did: key, value: identity });
  }

  async loadSession(did: string): Promise<Session> {
    const key = sanitizeDid(did);
    const row = await this.db.sessions.get(key);
    if (!row) throw new StorageError(`no session for ${did}`);
    return row.value;
  }

  async saveSession(did: string, session: Session): Promise<void> {
    const key = sanitizeDid(did);
    await this.db.sessions.put({ did: key, value: session });
  }

  async clearSession(did: string): Promise<void> {
    const key = sanitizeDid(did);
    await this.db.sessions.delete(key);
  }

  // -- Pair state -----------------------------------------------------------

  async savePairState(did: string, rkey: string, privateKey: Uint8Array): Promise<void> {
    const key = sanitizeDid(did);
    await this.db.pairStates.put({ did: key, rkey, privateKey });
  }

  async loadPairState(did: string, rkey: string): Promise<Uint8Array> {
    const key = sanitizeDid(did);
    const row = await this.db.pairStates.get([key, rkey]);
    if (!row) throw new StorageError(`no pair state for ${did}/${rkey}`);
    return row.privateKey;
  }

  async deletePairState(did: string, rkey: string): Promise<void> {
    const key = sanitizeDid(did);
    await this.db.pairStates.delete([key, rkey]);
  }

  // -- Cache: record-level --------------------------------------------------

  async cacheGetRecord<T>(
    did: string,
    collection: string,
    uri: string,
  ): Promise<CachedRecord<T> | null> {
    const row = await this.db.cacheRecords.get([did, collection, uri]);
    if (!row) return null;
    return { uri: row.uri, cid: row.cid, value: row.value as T };
  }

  async cachePutRecords<T>(
    did: string,
    collection: string,
    records: readonly CachedRecord<T>[],
  ): Promise<void> {
    const rows = records.map((r) => ({
      did,
      collection,
      uri: r.uri,
      cid: r.cid,
      value: r.value,
    }));
    await this.db.cacheRecords.bulkPut(rows);
  }

  async cacheRemoveRecord(did: string, collection: string, uri: string): Promise<void> {
    await this.db.cacheRecords.delete([did, collection, uri]);
  }

  // -- Cache: collection-level ----------------------------------------------

  async cacheGetCollection<T>(
    did: string,
    collection: string,
  ): Promise<CachedCollection<T> | null> {
    const [meta, rows] = await Promise.all([
      this.db.cacheMeta.get([did, collection]),
      this.db.cacheRecords.where("[did+collection]").equals([did, collection]).toArray(),
    ]);
    if (!meta) return null;
    const records = rows.map((r) => ({ uri: r.uri, cid: r.cid, value: r.value as T }));
    return { records, fetched_at: meta.fetchedAt };
  }

  async cachePutCollection<T>(
    did: string,
    collection: string,
    data: CachedCollection<T>,
  ): Promise<void> {
    await this.db.transaction("rw", [this.db.cacheRecords, this.db.cacheMeta], async () => {
      await this.db.cacheRecords.where("[did+collection]").equals([did, collection]).delete();
      const rows = data.records.map((r) => ({
        did,
        collection,
        uri: r.uri,
        cid: r.cid,
        value: r.value,
      }));
      await this.db.cacheRecords.bulkPut(rows);
      await this.db.cacheMeta.put({ did, collection, fetchedAt: data.fetched_at });
    });
  }

  async cacheInvalidateCollection(did: string, collection: string): Promise<void> {
    await this.db.cacheMeta.delete([did, collection]);
  }

  // -- Cache: account-level -------------------------------------------------

  async cacheClear(did: string): Promise<void> {
    await this.db.transaction("rw", [this.db.cacheRecords, this.db.cacheMeta], async () => {
      await this.db.cacheRecords.where("did").equals(did).delete();
      await this.db.cacheMeta.where("did").equals(did).delete();
    });
  }

  // -- Account removal ------------------------------------------------------

  async removeAccount(did: string): Promise<void> {
    const config = await this.loadConfig();
    const remainingAccounts = Object.fromEntries(
      Object.entries(config.accounts).filter(([key]) => key !== did),
    );
    const remaining = Object.keys(remainingAccounts);
    const updatedConfig: Config = {
      ...config,
      accounts: remainingAccounts,
      default_did:
        config.default_did === did
          ? remaining.length > 0
            ? remaining[0]
            : undefined
          : config.default_did,
    };
    const key = sanitizeDid(did);
    await this.db.transaction(
      "rw",
      [
        this.db.configs,
        this.db.identities,
        this.db.sessions,
        this.db.pairStates,
        this.db.cacheRecords,
        this.db.cacheMeta,
      ],
      async () => {
        await this.db.configs.put({ key: CONFIG_KEY, value: updatedConfig });
        await this.db.identities.delete(key);
        await this.db.sessions.delete(key);
        await this.db.pairStates.where("did").equals(key).delete();
        await this.db.cacheRecords.where("did").equals(did).delete();
        await this.db.cacheMeta.where("did").equals(did).delete();
      },
    );
  }

  /** Close the database connection. */
  close(): void {
    this.db.close();
  }
}
