// Worker context layer — bridges IndexedDB ↔ WASM OpakeContext.
//
// The worker holds a storage adapter and the current account DID.
// Each domain operation constructs a fresh OpakeContext from storage,
// gets a FileManager, does the work, and frees. Session persistence
// is automatic via signoff → JsStorage → IndexedDB.

import { IndexedDbStorage } from "@/lib/indexeddbStorage";
import { OpakeContext, type FileManager } from "@/wasm/opake-wasm/opake";

// ---------------------------------------------------------------------------
// Storage adapter — the JS object WASM calls into via JsStorage
// ---------------------------------------------------------------------------

export const storage = new IndexedDbStorage();

const storageAdapter = {
  loadConfig: () => storage.loadConfig(),
  saveConfig: (config: unknown) =>
    storage.saveConfig(config as Parameters<typeof storage.saveConfig>[0]),
  loadIdentity: (did: string) => storage.loadIdentity(did),
  saveIdentity: (did: string, identity: unknown) =>
    storage.saveIdentity(did, identity as Parameters<typeof storage.saveIdentity>[1]),
  loadSession: (did: string) => storage.loadSession(did),
  saveSession: (did: string, session: unknown) =>
    storage.saveSession(did, session as Parameters<typeof storage.saveSession>[1]),
  removeAccount: (did: string) => storage.removeAccount(did),
  // Cache methods — bridged to IndexedDB for local tree/document caching
  cacheGetCollection: (did: string, collection: string) =>
    storage.cacheGetCollection(did, collection),
  cachePutCollection: (did: string, collection: string, data: unknown) =>
    storage.cachePutCollection(
      did,
      collection,
      data as Parameters<typeof storage.cachePutCollection>[2],
    ),
  cacheGetRecord: (did: string, collection: string, uri: string) =>
    storage.cacheGetRecord(did, collection, uri),
  cachePutRecords: (did: string, collection: string, records: unknown) =>
    storage.cachePutRecords(
      did,
      collection,
      records as Parameters<typeof storage.cachePutRecords>[2],
    ),
  cacheRemoveRecord: (did: string, collection: string, uri: string) =>
    storage.cacheRemoveRecord(did, collection, uri),
  cacheInvalidateCollection: (did: string, collection: string) =>
    storage.cacheInvalidateCollection(did, collection),
  cacheClear: (did: string) => storage.cacheClear(did),
};

// ---------------------------------------------------------------------------
// Account state
// ---------------------------------------------------------------------------

const state = { did: null as string | null };

export function setAccount(did: string): void {
  console.info("[context] setAccount called:", did);
  state.did = did; // eslint-disable-line functional/immutable-data
  void import("./daemon")
    .then(({ startDaemon }) => {
      console.info("[context] daemon module loaded, calling startDaemon");
      startDaemon();
    })
    .catch((err: unknown) => console.error("[context] daemon import failed:", err));
}

export function clearAccount(): void {
  void import("./daemon").then(({ stopDaemon }) => stopDaemon());
  state.did = null; // eslint-disable-line functional/immutable-data
}

// ---------------------------------------------------------------------------
// Scoped helpers — construct OpakeContext per operation, free on exit
// ---------------------------------------------------------------------------

/**
 * Run a callback with a cabinet FileManager.
 * For personal file operations (upload, download, share, etc.)
 */
export async function withCabinet<T>(fn: (fm: FileManager) => Promise<T>): Promise<T> {
  const ctx = await OpakeContext.create(state.did, storageAdapter);
  const fm = ctx.cabinet();
  try {
    return await fn(fm);
  } finally {
    fm.free();
  }
}

/**
 * Run a callback with a workspace FileManager.
 * For workspace file operations (scoped to a specific keyring).
 */
export async function withWorkspace<T>(
  keyringUri: string,
  ownerDid: string,
  key: Uint8Array,
  rotation: bigint,
  fn: (fm: FileManager) => Promise<T>,
): Promise<T> {
  const ctx = await OpakeContext.create(state.did, storageAdapter);
  const fm = ctx.workspace(keyringUri, ownerDid, key, rotation);
  try {
    return await fn(fm);
  } finally {
    fm.free();
  }
}

/**
 * Run a callback with an OpakeContext directly.
 * For workspace management (create, list, add member, leave) and
 * cross-PDS operations that don't need a FileManager.
 */
export async function withOpake<T>(fn: (ctx: OpakeContext) => Promise<T>): Promise<T> {
  const ctx = await OpakeContext.create(state.did, storageAdapter);
  try {
    return await fn(ctx);
  } finally {
    ctx.free();
  }
}
