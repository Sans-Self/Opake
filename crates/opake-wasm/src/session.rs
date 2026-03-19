// WASM export for proactive session refresh (used by the Service Worker).

use opake_core::client::session_refresh::{
    proactive_refresh, RefreshOutcome, DEFAULT_REFRESH_THRESHOLD_SECONDS,
};
use opake_core::client::{time, Session, WasmTransport};
use opake_core::crypto::OsRng;
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = defaultRefreshThresholdSeconds)]
pub fn default_refresh_threshold_seconds() -> f64 {
    DEFAULT_REFRESH_THRESHOLD_SECONDS as f64
}

/// Check whether a session needs refreshing and, if so, refresh it.
///
/// Returns the updated session as a JS object, or `null` if no refresh was
/// needed. Throws on failure. The caller (Service Worker) is responsible for
/// persisting the returned session to IndexedDB.
#[wasm_bindgen(js_name = proactiveSessionRefresh)]
pub async fn proactive_session_refresh(
    session_js: JsValue,
    pds_url: &str,
    threshold_seconds: f64,
) -> Result<JsValue, JsError> {
    let session: Session =
        serde_wasm_bindgen::from_value(session_js).map_err(|e| JsError::new(&e.to_string()))?;
    let transport = WasmTransport::new();
    let now = time::unix_now();

    let result = proactive_refresh(
        &transport,
        &session,
        pds_url,
        threshold_seconds as i64,
        now,
        &mut OsRng,
    )
    .await;

    match result {
        RefreshOutcome::Refreshed(new_session) => {
            let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
            new_session
                .serialize(&serializer)
                .map_err(|e| JsError::new(&e.to_string()))
        }
        RefreshOutcome::NotNeeded => Ok(JsValue::NULL),
        RefreshOutcome::Failed(e) => Err(JsError::new(&e.to_string())),
    }
}
