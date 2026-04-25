// JsStorage — Storage trait implementation backed by JS callbacks.
//
// The JS side provides an object with async methods (loadConfig, saveSession,
// etc.) that read/write IndexedDB. This module bridges those callbacks into
// opake-core's Storage trait so Opake can read identity, session, and config
// the same way the CLI reads from the filesystem.

use js_sys::Promise;
use opake_core::client::Session;
use opake_core::error::Error;
use opake_core::storage::{CachedCollection, CachedRecord, Config, Identity, Storage};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

// ---------------------------------------------------------------------------
// JS-side adapter (imported via wasm_bindgen)
// ---------------------------------------------------------------------------

#[wasm_bindgen]
extern "C" {
    /// JS object implementing the Storage contract.
    ///
    /// The worker creates this by wrapping its IndexedDbStorage instance:
    /// ```js
    /// const adapter = {
    ///   loadConfig: () => storage.loadConfig(),
    ///   loadIdentity: (did) => storage.loadIdentity(did),
    ///   saveSession: (did, session) => storage.saveSession(did, session),
    ///   // ...
    /// };
    /// ```
    pub type JsStorageAdapter;

    #[wasm_bindgen(method, js_name = loadConfig)]
    fn load_config_js(this: &JsStorageAdapter) -> Promise;

    #[wasm_bindgen(method, js_name = saveConfig)]
    fn save_config_js(this: &JsStorageAdapter, config: JsValue) -> Promise;

    #[wasm_bindgen(method, js_name = loadIdentity)]
    fn load_identity_js(this: &JsStorageAdapter, did: &str) -> Promise;

    #[wasm_bindgen(method, js_name = saveIdentity)]
    fn save_identity_js(this: &JsStorageAdapter, did: &str, identity: JsValue) -> Promise;

    #[wasm_bindgen(method, js_name = loadSession)]
    fn load_session_js(this: &JsStorageAdapter, did: &str) -> Promise;

    #[wasm_bindgen(method, js_name = saveSession)]
    fn save_session_js(this: &JsStorageAdapter, did: &str, session: JsValue) -> Promise;

    #[wasm_bindgen(method, js_name = removeAccount)]
    fn remove_account_js(this: &JsStorageAdapter, did: &str) -> Promise;

    #[wasm_bindgen(method, js_name = savePairState)]
    fn save_pair_state_js(
        this: &JsStorageAdapter,
        did: &str,
        rkey: &str,
        private_key: &[u8],
    ) -> Promise;

    #[wasm_bindgen(method, js_name = loadPairState)]
    fn load_pair_state_js(this: &JsStorageAdapter, did: &str, rkey: &str) -> Promise;

    #[wasm_bindgen(method, js_name = deletePairState)]
    fn delete_pair_state_js(this: &JsStorageAdapter, did: &str, rkey: &str) -> Promise;

    #[wasm_bindgen(method, js_name = cacheGetCollection)]
    fn cache_get_collection_js(this: &JsStorageAdapter, did: &str, collection: &str) -> Promise;

    #[wasm_bindgen(method, js_name = cachePutCollection)]
    fn cache_put_collection_js(
        this: &JsStorageAdapter,
        did: &str,
        collection: &str,
        data: JsValue,
    ) -> Promise;

    #[wasm_bindgen(method, js_name = cacheGetRecord)]
    fn cache_get_record_js(
        this: &JsStorageAdapter,
        did: &str,
        collection: &str,
        uri: &str,
    ) -> Promise;

    #[wasm_bindgen(method, js_name = cachePutRecords)]
    fn cache_put_records_js(
        this: &JsStorageAdapter,
        did: &str,
        collection: &str,
        records: JsValue,
    ) -> Promise;

    #[wasm_bindgen(method, js_name = cacheRemoveRecord)]
    fn cache_remove_record_js(
        this: &JsStorageAdapter,
        did: &str,
        collection: &str,
        uri: &str,
    ) -> Promise;

    #[wasm_bindgen(method, js_name = cacheInvalidateCollection)]
    fn cache_invalidate_collection_js(
        this: &JsStorageAdapter,
        did: &str,
        collection: &str,
    ) -> Promise;

    #[wasm_bindgen(method, js_name = cacheClear)]
    fn cache_clear_js(this: &JsStorageAdapter, did: &str) -> Promise;

}

// ---------------------------------------------------------------------------
// JsStorage
// ---------------------------------------------------------------------------

pub struct JsStorage {
    adapter: JsStorageAdapter,
}

impl JsStorage {
    pub fn new(adapter: JsStorageAdapter) -> Self {
        Self { adapter }
    }
}

impl Storage for JsStorage {
    async fn load_config(&self) -> Result<Config, Error> {
        let val = resolve(&self.adapter.load_config_js()).await?;
        from_js(val)
    }

    async fn save_config(&self, config: &Config) -> Result<(), Error> {
        let js_val = to_js(config)?;
        resolve(&self.adapter.save_config_js(js_val)).await?;
        Ok(())
    }

    async fn load_identity(&self, did: &str) -> Result<Identity, Error> {
        let val = resolve(&self.adapter.load_identity_js(did)).await?;
        from_js(val)
    }

    async fn save_identity(&self, did: &str, identity: &Identity) -> Result<(), Error> {
        let js_val = to_js(identity)?;
        resolve(&self.adapter.save_identity_js(did, js_val)).await?;
        Ok(())
    }

    async fn load_session(&self, did: &str) -> Result<Session, Error> {
        let val = resolve(&self.adapter.load_session_js(did)).await?;
        from_js(val)
    }

    async fn save_session(&self, did: &str, session: &Session) -> Result<(), Error> {
        let js_val = to_js(session)?;
        resolve(&self.adapter.save_session_js(did, js_val)).await?;
        Ok(())
    }

    async fn remove_account(&self, did: &str) -> Result<(), Error> {
        resolve(&self.adapter.remove_account_js(did)).await?;
        Ok(())
    }

    // -- Pair state: bridged to JS-side IndexedDB ------------------------------

    async fn save_pair_state(
        &self,
        did: &str,
        rkey: &str,
        private_key: &[u8],
    ) -> Result<(), Error> {
        resolve(&self.adapter.save_pair_state_js(did, rkey, private_key)).await?;
        Ok(())
    }

    async fn load_pair_state(&self, did: &str, rkey: &str) -> Result<Vec<u8>, Error> {
        let val = resolve(&self.adapter.load_pair_state_js(did, rkey)).await?;
        if val.is_null() || val.is_undefined() {
            return Err(Error::NotFound(format!("pair state {rkey}")));
        }
        // Raw Uint8Array, not a serde shape — bypass `from_js` and copy bytes.
        Ok(js_sys::Uint8Array::new(&val).to_vec())
    }

    async fn delete_pair_state(&self, did: &str, rkey: &str) -> Result<(), Error> {
        resolve(&self.adapter.delete_pair_state_js(did, rkey)).await?;
        Ok(())
    }

    // -- Cache: bridged to JS IndexedDB ----------------------------------------

    async fn cache_get_record(
        &self,
        did: &str,
        collection: &str,
        uri: &str,
    ) -> Result<Option<CachedRecord>, Error> {
        let val = resolve(&self.adapter.cache_get_record_js(did, collection, uri)).await?;
        if val.is_null() || val.is_undefined() {
            return Ok(None);
        }
        Ok(Some(from_js(val)?))
    }

    async fn cache_put_records(
        &self,
        did: &str,
        collection: &str,
        records: &[CachedRecord],
    ) -> Result<(), Error> {
        let js_val = to_js(&records)?;
        resolve(&self.adapter.cache_put_records_js(did, collection, js_val)).await?;
        Ok(())
    }

    async fn cache_remove_record(
        &self,
        did: &str,
        collection: &str,
        uri: &str,
    ) -> Result<(), Error> {
        resolve(&self.adapter.cache_remove_record_js(did, collection, uri)).await?;
        Ok(())
    }

    async fn cache_get_collection(
        &self,
        did: &str,
        collection: &str,
    ) -> Result<Option<CachedCollection>, Error> {
        let val = resolve(&self.adapter.cache_get_collection_js(did, collection)).await?;
        if val.is_null() || val.is_undefined() {
            return Ok(None);
        }
        Ok(Some(from_js(val)?))
    }

    async fn cache_put_collection(
        &self,
        did: &str,
        collection: &str,
        data: &CachedCollection,
    ) -> Result<(), Error> {
        let js_val = to_js(data)?;
        resolve(
            &self
                .adapter
                .cache_put_collection_js(did, collection, js_val),
        )
        .await?;
        Ok(())
    }

    async fn cache_invalidate_collection(&self, did: &str, collection: &str) -> Result<(), Error> {
        resolve(&self.adapter.cache_invalidate_collection_js(did, collection)).await?;
        Ok(())
    }

    async fn cache_clear(&self, did: &str) -> Result<(), Error> {
        resolve(&self.adapter.cache_clear_js(did)).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn resolve(promise: &Promise) -> Result<JsValue, Error> {
    JsFuture::from(promise.clone())
        .await
        .map_err(js_storage_err)
}

fn from_js<T: serde::de::DeserializeOwned>(val: JsValue) -> Result<T, Error> {
    serde_wasm_bindgen::from_value(val).map_err(|e| Error::Storage(e.to_string()))
}

fn to_js<T: serde::Serialize>(val: &T) -> Result<JsValue, Error> {
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    val.serialize(&serializer)
        .map_err(|e| Error::Storage(e.to_string()))
}

fn js_storage_err(e: JsValue) -> Error {
    let message = e
        .as_string()
        .or_else(|| {
            js_sys::Reflect::get(&e, &"message".into())
                .ok()?
                .as_string()
        })
        .unwrap_or_else(|| format!("{e:?}"));
    Error::Storage(message)
}
