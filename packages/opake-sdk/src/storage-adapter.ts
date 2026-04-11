// Bridge between the SDK's Storage interface and the WASM JsStorageAdapter.
//
// The WASM layer imports an extern type with specific method names. This
// typed interface mirrors the Rust JsStorageAdapter extern — if WASM adds or
// renames a method, TS compilation will catch the mismatch here.

import type { Storage, Config, Identity, Session, CachedRecord, CachedCollection } from "./storage";

/**
 * Typed interface matching the Rust `JsStorageAdapter` extern in js_storage.rs.
 *
 * Every method here corresponds to a `#[wasm_bindgen(method)]` import on the
 * Rust side. If WASM calls a method not listed here, TS will catch it at the
 * call site (the return value is typed, not `Record<string, unknown>`).
 */
// Note: Storage.clearSession is intentionally omitted — WASM never clears
// sessions directly (the SDK handles that at the JS level via removeAccount
// or direct IDB operations).
export interface WasmStorageAdapter {
  loadConfig(): Promise<Config>;
  saveConfig(config: Config): Promise<void>;
  loadIdentity(did: string): Promise<Identity>;
  saveIdentity(did: string, identity: Identity): Promise<void>;
  loadSession(did: string): Promise<Session>;
  saveSession(did: string, session: Session): Promise<void>;
  removeAccount(did: string): Promise<void>;
  cacheGetRecord(did: string, collection: string, uri: string): Promise<CachedRecord | null>;
  cachePutRecords(did: string, collection: string, records: readonly CachedRecord[]): Promise<void>;
  cacheRemoveRecord(did: string, collection: string, uri: string): Promise<void>;
  cacheGetCollection(did: string, collection: string): Promise<CachedCollection | null>;
  cachePutCollection(did: string, collection: string, data: CachedCollection): Promise<void>;
  cacheInvalidateCollection(did: string, collection: string): Promise<void>;
  cacheClear(did: string): Promise<void>;
}

/**
 * Create the storage adapter object that the WASM JsStorageAdapter expects.
 */
export function createStorageAdapter(storage: Storage): WasmStorageAdapter {
  return {
    loadConfig: () => storage.loadConfig(),
    saveConfig: (config) => storage.saveConfig(config),
    loadIdentity: (did) => storage.loadIdentity(did),
    saveIdentity: (did, identity) => storage.saveIdentity(did, identity),
    loadSession: (did) => storage.loadSession(did),
    saveSession: (did, session) => storage.saveSession(did, session),
    removeAccount: (did) => storage.removeAccount(did),
    cacheGetRecord: (did, collection, uri) => storage.cacheGetRecord(did, collection, uri),
    cachePutRecords: (did, collection, records) =>
      storage.cachePutRecords(did, collection, records),
    cacheRemoveRecord: (did, collection, uri) => storage.cacheRemoveRecord(did, collection, uri),
    cacheGetCollection: (did, collection) => storage.cacheGetCollection(did, collection),
    cachePutCollection: (did, collection, data) =>
      storage.cachePutCollection(did, collection, data),
    cacheInvalidateCollection: (did, collection) =>
      storage.cacheInvalidateCollection(did, collection),
    cacheClear: (did) => storage.cacheClear(did),
  };
}
