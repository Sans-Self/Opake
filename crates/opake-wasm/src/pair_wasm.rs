// New-device pair flow bindings (identity-less).
//
// These functions don't require a constructed OpakeContext — the new
// device has a session but no encryption identity until pairing completes.
// The ephemeral private key stays inside WASM: `createPairRequest` persists
// it to the caller's Storage adapter, `tryCompletePair` reads it back. JS
// only ever sees `{ uri, rkey, ephemeralPublicKey }` going out and `true`
// coming back on completion.

use opake_core::client::{Transport, XrpcClient, WasmTransport};
use opake_core::crypto::OsRng;
use opake_core::error::Error;
use opake_core::opake::authenticated_client;
use opake_core::storage::Storage;
use wasm_bindgen::prelude::*;

use crate::js_storage::{JsStorage, JsStorageAdapter};
use crate::wasm_util::{to_js, wasm_err};

/// Create a pair request on the caller's PDS.
///
/// Writes the ephemeral private key to the supplied Storage — it never
/// crosses back into JS. Callers display the returned public key
/// fingerprint on the new device for out-of-band comparison, then poll
/// `tryCompletePair` until the paired device approves.
#[wasm_bindgen(js_name = createPairRequest)]
pub async fn create_pair_request_js(
    did: String,
    storage_adapter: JsStorageAdapter,
) -> Result<JsValue, JsError> {
    let storage = JsStorage::new(storage_adapter);
    let mut client = authenticated_client(&storage, &did, WasmTransport::new())
        .await
        .map_err(wasm_err)?;

    let now = opake_core::timestamp::rfc3339_from_micros(crate::now_micros());
    let mut rng = OsRng;
    let info = opake_core::pairing::create_pair_request(&mut client, &storage, &did, &now, &mut rng)
        .await
        .map_err(wasm_err)?;

    persist_if_refreshed(&storage, &did, &client).await?;

    // Public halves only — the X25519 and ML-KEM-768 ephemeral *public*
    // keys cross to JS for fingerprint display. The matching private
    // keys stay inside WASM-owned Storage (persisted by
    // `create_pair_request` above) and never leave the boundary. The
    // SDK consumes `PairRequestResult.ts` (generated) and transforms
    // snake_case → camelCase via `pairRequestResultSchema`.
    to_js(&crate::bindings::PairRequestResultDto {
        uri: info.uri,
        rkey: info.rkey,
        x25519_ephemeral_public_key: info.x25519_ephemeral_public_key.to_vec(),
        ml_kem_ephemeral_public_key: info.ml_kem_ephemeral_public_key.to_vec(),
    })
}

/// Poll once for a pair response matching `request_rkey`.
///
/// Returns `true` if a response was found and pairing completed — the
/// Identity is now persisted to Storage and `Opake.init` will succeed.
/// Returns `false` if nothing matched yet; the SDK's `awaitPairCompletion`
/// wraps this in a `setTimeout` loop.
#[wasm_bindgen(js_name = tryCompletePair)]
pub async fn try_complete_pair_js(
    did: String,
    storage_adapter: JsStorageAdapter,
    request_rkey: String,
) -> Result<bool, JsError> {
    let storage = JsStorage::new(storage_adapter);
    let mut client = authenticated_client(&storage, &did, WasmTransport::new())
        .await
        .map_err(wasm_err)?;

    let result =
        opake_core::pairing::try_complete_pair(&mut client, &storage, &did, &request_rkey)
            .await
            .map_err(wasm_err)?;

    persist_if_refreshed(&storage, &did, &client).await?;
    Ok(result)
}

/// Cancel an in-flight pair request. Wipes the ephemeral key from Storage
/// and removes the request record from the PDS (tolerant of either side
/// already being gone).
#[wasm_bindgen(js_name = cancelPairRequest)]
pub async fn cancel_pair_request_js(
    did: String,
    storage_adapter: JsStorageAdapter,
    request_rkey: String,
) -> Result<(), JsError> {
    let storage = JsStorage::new(storage_adapter);
    let mut client = authenticated_client(&storage, &did, WasmTransport::new())
        .await
        .map_err(wasm_err)?;

    opake_core::pairing::cancel_pair_request(&mut client, &storage, &did, &request_rkey)
        .await
        .map_err(wasm_err)?;

    persist_if_refreshed(&storage, &did, &client).await
}

async fn persist_if_refreshed<T: Transport>(
    storage: &JsStorage,
    did: &str,
    client: &XrpcClient<T>,
) -> Result<(), JsError> {
    if client.session_refreshed() {
        if let Some(session) = client.session() {
            storage
                .save_session(did, session)
                .await
                .map_err(|e: Error| wasm_err(e))?;
        }
    }
    Ok(())
}
