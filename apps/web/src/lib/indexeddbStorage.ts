// IndexedDB-backed Storage implementation using Dexie.js.
// Mirrors: crates/opake-cli/src/config.rs — FileStorage (but for the browser)

import Dexie, { type EntityTable, type Table } from "dexie";
import type { CachedCollection, CachedRecord, Config, Identity, Session } from "./storageTypes";
import { type Storage, StorageError, sanitizeDid } from "./storage";
import { ConfigSchema, IdentitySchema, SessionSchema, CachedProfileSchema } from "./schemas";

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

export interface CachedProfile {
  readonly avatarUrl: string | null;
  readonly bannerUrl: string | null;
  readonly fetchedAt: number;
}

interface ProfileRow {
  did: string;
  value: CachedProfile;
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

interface TaskRow {
  id: string;
  value: unknown;
}

// ---------------------------------------------------------------------------
// Database
// ---------------------------------------------------------------------------

class OpakeDatabase extends Dexie {
  readonly configs!: Readonly<EntityTable<ConfigRow, "key">>;
  readonly identities!: Readonly<EntityTable<IdentityRow, "did">>;
  readonly sessions!: Readonly<EntityTable<SessionRow, "did">>;
  readonly profiles!: Readonly<EntityTable<ProfileRow, "did">>;
  readonly cacheRecords!: Readonly<Table<CacheRecordRow>>;
  readonly cacheMeta!: Readonly<Table<CacheMetaRow>>;
  readonly tasks!: Readonly<EntityTable<TaskRow, "id">>;

  constructor(name = "opake") {
    super(name);
    this.version(1).stores({
      configs: "key",
      identities: "did",
      sessions: "did",
    });
    this.version(2).stores({
      configs: "key",
      identities: "did",
      sessions: "did",
      profiles: "did",
    });
    this.version(3).stores({
      configs: "key",
      identities: "did",
      sessions: "did",
      profiles: "did",
      cacheRecords: "[did+collection+uri], [did+collection], did",
      cacheMeta: "[did+collection], did",
    });
    this.version(4).stores({
      configs: "key",
      identities: "did",
      sessions: "did",
      profiles: "did",
      cacheRecords: "[did+collection+uri], [did+collection], did",
      cacheMeta: "[did+collection], did",
      tasks: "id",
    });
  }
}

// ---------------------------------------------------------------------------
// IndexedDbStorage
// ---------------------------------------------------------------------------

export class IndexedDbStorage implements Storage {
  private readonly db: Readonly<OpakeDatabase>;

  constructor(dbName = "opake") {
    this.db = new OpakeDatabase(dbName);
  }

  // -- Config / Identity / Session ------------------------------------------

  async loadConfig(): Promise<Config> {
    const row = await this.db.configs.get(CONFIG_KEY);
    if (!row) {
      throw new StorageError("no config found — log in first");
    }
    return ConfigSchema.parse(row.value);
  }

  async saveConfig(config: Config): Promise<void> {
    await this.db.configs.put({ key: CONFIG_KEY, value: config });
  }

  async loadIdentity(did: string): Promise<Identity> {
    const key = sanitizeDid(did);
    const row = await this.db.identities.get(key);
    if (!row) {
      throw new StorageError(`no identity for ${did} — log in first`);
    }
    return IdentitySchema.parse(row.value);
  }

  async saveIdentity(did: string, identity: Identity): Promise<void> {
    const key = sanitizeDid(did);
    await this.db.identities.put({ did: key, value: identity });
  }

  async loadSession(did: string): Promise<Session> {
    const key = sanitizeDid(did);
    const row = await this.db.sessions.get(key);
    if (!row) {
      throw new StorageError(`no session for ${did}`);
    }
    return SessionSchema.parse(row.value);
  }

  async saveSession(did: string, session: Session): Promise<void> {
    const key = sanitizeDid(did);
    await this.db.sessions.put({ did: key, value: session });
  }

  async deleteSession(did: string): Promise<void> {
    const key = sanitizeDid(did);
    await this.db.sessions.delete(key);
  }

  // -- Profiles (not on trait — web-only) -----------------------------------

  async loadProfile(did: string): Promise<CachedProfile | null> {
    const key = sanitizeDid(did);
    const row = await this.db.profiles.get(key);
    if (!row) return null;
    return CachedProfileSchema.parse(row.value);
  }

  async saveProfile(did: string, profile: CachedProfile): Promise<void> {
    const key = sanitizeDid(did);
    await this.db.profiles.put({ did: key, value: profile });
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
    const rows: readonly CacheRecordRow[] = records.map((r) => ({
      did,
      collection,
      uri: r.uri,
      cid: r.cid,
      value: r.value,
    }));
    await this.db.cacheRecords.bulkPut(rows as CacheRecordRow[]);
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

    const records: readonly CachedRecord<T>[] = rows.map((r) => ({
      uri: r.uri,
      cid: r.cid,
      value: r.value as T,
    }));

    return { records, fetched_at: meta.fetchedAt };
  }

  async cachePutCollection<T>(
    did: string,
    collection: string,
    data: CachedCollection<T>,
  ): Promise<void> {
    await this.db.transaction("rw", [this.db.cacheRecords, this.db.cacheMeta], async () => {
      // Delete existing records for this (did, collection)
      await this.db.cacheRecords.where("[did+collection]").equals([did, collection]).delete();

      // Insert fresh records
      const rows: readonly CacheRecordRow[] = data.records.map((r) => ({
        did,
        collection,
        uri: r.uri,
        cid: r.cid,
        value: r.value,
      }));
      await this.db.cacheRecords.bulkPut(rows as CacheRecordRow[]);

      // Set collection metadata
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

  // -- Account removal (extends to clear cache) -----------------------------

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
        this.db.profiles,
        this.db.cacheRecords,
        this.db.cacheMeta,
      ],
      async () => {
        await this.db.configs.put({ key: CONFIG_KEY, value: updatedConfig });
        await this.db.identities.delete(key);
        await this.db.sessions.delete(key);
        await this.db.profiles.delete(key);
        // Clear all cached records for this account
        await this.db.cacheRecords.where("did").equals(did).delete();
        await this.db.cacheMeta.where("did").equals(did).delete();
      },
    );
  }

  // -- Tasks: daemon background task persistence -----------------------------

  async saveTask(task: unknown): Promise<void> {
    const t = task as { id: string };
    await this.db.tasks.put({ id: t.id, value: task });
  }

  async loadTasks(): Promise<unknown[]> {
    const rows = await this.db.tasks.reverse().toArray();
    return rows.map((r) => r.value);
  }

  async deleteTask(id: string): Promise<void> {
    await this.db.tasks.delete(id);
  }

  /** Close the database connection. Useful for test cleanup. */
  close(): void {
    this.db.close();
  }

  /** Delete the entire database. Useful for test cleanup. */
  async destroy(): Promise<void> {
    this.db.close();
    await this.db.delete();
  }
}

/** Shared singleton — every module should import this instead of constructing its own. */
export const storage = new IndexedDbStorage();
