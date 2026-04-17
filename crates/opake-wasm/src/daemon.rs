// WASM exports for Service Worker maintenance tasks:
// session refresh, pair request cleanup, stale grant healing.

use opake_core::client::session_refresh::{
    proactive_refresh, RefreshOutcome, DEFAULT_REFRESH_THRESHOLD_SECONDS,
};
use opake_core::client::{time, Session, WasmTransport};
use opake_core::crypto::OsRng;
use opake_core::pairing::DEFAULT_PAIR_REQUEST_TTL_SECONDS;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use opake_core::indexer::daemon;

use crate::wasm_util;

// ---------------------------------------------------------------------------
// Constants (exported so the Service Worker uses the same values as the CLI)
// ---------------------------------------------------------------------------

/// Returns the daemon task registry as a JSON array:
/// `[{ name, intervalSeconds, description }, ...]`
#[wasm_bindgen(js_name = daemonTaskDefs)]
pub fn daemon_task_defs() -> Result<JsValue, JsError> {
    #[derive(serde::Serialize)]
    struct TaskDefJs {
        name: &'static str,
        interval_seconds: i64,
        description: &'static str,
    }

    let defs: Vec<TaskDefJs> = daemon::TASKS
        .iter()
        .map(|t| TaskDefJs {
            name: t.name,
            interval_seconds: t.interval_seconds,
            description: t.description,
        })
        .collect();

    serde_wasm_bindgen::to_value(&defs).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen(js_name = defaultRefreshThresholdSeconds)]
pub fn default_refresh_threshold_seconds() -> f64 {
    daemon::SESSION_REFRESH_THRESHOLD as f64
}

#[wasm_bindgen(js_name = defaultPairRequestTtlSeconds)]
pub fn default_pair_request_ttl_seconds() -> f64 {
    DEFAULT_PAIR_REQUEST_TTL_SECONDS as f64
}

#[wasm_bindgen(js_name = defaultPendingShareTtlSeconds)]
pub fn default_pending_share_ttl_seconds() -> f64 {
    opake_core::sharing::DEFAULT_PENDING_SHARE_TTL_SECONDS as f64
}

// ---------------------------------------------------------------------------
// Session refresh
// ---------------------------------------------------------------------------

/// Check whether a session needs refreshing and, if so, refresh it.
///
/// Returns the updated session as a JS object, or `null` if no refresh was
/// needed. Throws on failure.
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

// ---------------------------------------------------------------------------
// Pair request cleanup
// ---------------------------------------------------------------------------

/// Delete expired pair requests and orphaned pair responses.
///
/// Returns `{ result: { requestsDeleted, responsesDeleted }, session }`.
/// The caller must persist the returned session (DPoP nonce or token may
/// have been updated during the XRPC calls).
#[wasm_bindgen(js_name = cleanupExpiredPairRequests)]
pub async fn cleanup_expired_pair_requests_js(
    session_js: JsValue,
    pds_url: &str,
    ttl_seconds: f64,
) -> Result<JsValue, JsError> {
    let mut client = wasm_util::make_client(pds_url, session_js)?;
    let now = time::unix_now();

    let result =
        opake_core::pairing::cleanup_expired_pair_requests(&mut client, now, ttl_seconds as i64)
            .await
            .map_err(|e| JsError::new(&e.to_string()))?;

    wasm_util::result_with_session(&client, &result)
}

// ---------------------------------------------------------------------------
// Stale grant healing
// ---------------------------------------------------------------------------

/// Check all grants and delete any whose recipient has no valid public key.
///
/// Returns `{ result: { grantsChecked, grantsDeleted, grantsFailed }, session }`.
/// The caller must persist the returned session.
#[wasm_bindgen(js_name = healStaleGrants)]
pub async fn heal_stale_grants_js(session_js: JsValue, pds_url: &str) -> Result<JsValue, JsError> {
    let mut client = wasm_util::make_client(pds_url, session_js)?;

    let result = opake_core::sharing::heal_stale_grants(&mut client)
        .await
        .map_err(|e| JsError::new(&e.to_string()))?;

    wasm_util::result_with_session(&client, &result)
}

// ---------------------------------------------------------------------------
// Pending share retry
// ---------------------------------------------------------------------------

/// Retry all pending shares for the authenticated account.
///
/// Returns `{ result: { checked, completed, expired, stillPending, failed }, session }`.
/// The caller must persist the returned session.
#[wasm_bindgen(js_name = retryPendingShares)]
pub async fn retry_pending_shares_js(
    session_js: JsValue,
    pds_url: &str,
    owner_did: &str,
    private_key: &[u8],
    ttl_seconds: f64,
) -> Result<JsValue, JsError> {
    use opake_core::sharing::{retry_pending_shares, RetryParams};

    let privkey = wasm_util::priv_key_from_slice(private_key)?;
    let mut client = wasm_util::make_client(pds_url, session_js)?;
    let transport = opake_core::client::WasmTransport::new();
    let now = time::unix_now();

    let params = RetryParams {
        caller_pds_url: pds_url,
        owner_did,
        owner_private_key: &privkey,
        now,
        ttl_seconds: ttl_seconds as i64,
    };

    let result = retry_pending_shares(&mut client, &transport, &params, &mut OsRng)
        .await
        .map_err(|e| JsError::new(&e.to_string()))?;

    wasm_util::result_with_session(&client, &result)
}
