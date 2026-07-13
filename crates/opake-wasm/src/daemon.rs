// WASM exports for the daemon task registry + related constants.
//
// Maintenance operations themselves (session refresh, pair-request
// cleanup, stale-grant healing, pending-share retry) live on
// `OpakeContext` — the SDK calls `opake.cleanupExpiredPairRequests()`,
// `opake.healStaleGrants()`, `opake.retryPendingShares()`, and
// `opake.proactiveRefresh()` directly. This module is intentionally
// narrow: the shared task registry the daemon reads, and the interval
// constants the CLI (committed runner) and the web timer (opportunistic
// runner) agree on. There is no service-worker runner — group keys never
// leave page-WASM, which disqualifies it (see docs/BACKGROUND_WORK.md).

use opake_core::indexer::daemon;
use opake_core::pairing::DEFAULT_PAIR_REQUEST_TTL_SECONDS;
use wasm_bindgen::prelude::*;

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
