// OAuth login flows — WASM exports.
//
// All token handling, DPoP key generation, and session construction happens
// here in WASM. JS never sees token responses or constructs session objects.
//
// The one exception: PendingLogin state crosses the boundary because it must
// survive a full-page redirect via sessionStorage. This includes the DPoP key
// and PKCE verifier. Once completeOAuthLogin is called, those values enter
// WASM and the resulting session (tokens, keys) never leaves.

use opake_core::client::dpop::DpopKeyPair;
use opake_core::client::oauth_discovery::generate_pkce;
use opake_core::client::oauth_token;
use opake_core::client::{OAuthSession, Session, WasmTransport, XrpcClient};
use opake_core::crypto::OsRng;
use opake_core::resolve::resolve_pds_for_login;
use opake_core::storage::{AccountEntry, Storage};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::js_storage::{JsStorage, JsStorageAdapter};
use crate::wasm_util::wasm_err;

// ---------------------------------------------------------------------------
// PendingLogin — serializable state that survives page redirects
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingLoginState {
    pds_url: String,
    did: String,
    handle: String,
    dpop_key: DpopKeyPair,
    pkce_verifier: String,
    csrf_state: String,
    token_endpoint: String,
    client_id: String,
    dpop_nonce: Option<String>,
}

// ---------------------------------------------------------------------------
// CSRF state generation
// ---------------------------------------------------------------------------

fn generate_csrf_state(rng: &mut OsRng) -> String {
    use opake_core::crypto::RngCore;

    let mut bytes = [0u8; 16];
    rng.fill_bytes(&mut bytes);
    // Hex is URL-safe without encoding and just as random as base64url.
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Start OAuth login
// ---------------------------------------------------------------------------

/// Start an OAuth login flow. Handles resolution, discovery, PKCE, DPoP
/// keypair generation, and the Pushed Authorization Request.
///
/// Returns `{ authUrl, pending }` where `pending` is serializable state
/// the caller saves to sessionStorage for the redirect round-trip.
#[wasm_bindgen(js_name = startOAuthLogin)]
pub async fn start_oauth_login(handle: &str, redirect_uri: &str) -> Result<JsValue, JsError> {
    let transport = WasmTransport::new();
    let mut rng = OsRng;

    // 1. Resolve handle → DID + PDS URL
    let (did, pds_url, resolved_handle) = resolve_pds_for_login(&transport, handle)
        .await
        .map_err(wasm_err)?;
    let handle_str = resolved_handle.unwrap_or_else(|| handle.to_string());

    // 2. Discover authorization server
    let (_prm, asm) =
        opake_core::client::oauth_discovery::discover_authorization_server(&transport, &pds_url)
            .await
            .map_err(wasm_err)?;

    // 3. Generate DPoP keypair + PKCE + CSRF state + scope
    let dpop_key = DpopKeyPair::generate(&mut rng);
    let pkce = generate_pkce(&mut rng);
    let csrf_state = generate_csrf_state(&mut rng);
    let scope = opake_core::scope::oauth_scope();
    let client_id = oauth_token::build_client_id(redirect_uri, &scope);

    // 4. Pushed Authorization Request (with DPoP proof + nonce retry)
    let par_endpoint = asm.par_endpoint();
    let timestamp = opake_core::client::time::unix_now();
    let mut dpop_nonce: Option<String> = None;
    let par_response = oauth_token::pushed_authorization_request(
        &transport,
        &par_endpoint,
        &client_id,
        redirect_uri,
        &pkce,
        &scope,
        &csrf_state,
        Some(handle),
        &dpop_key,
        &mut dpop_nonce,
        timestamp,
        &mut rng,
    )
    .await
    .map_err(wasm_err)?;

    // 5. Build authorization URL
    let auth_url = oauth_token::build_authorization_url(
        &asm.authorization_endpoint,
        &client_id,
        &par_response.request_uri,
    );

    // 6. Return auth URL + serializable pending state
    // Config is NOT saved here — the user hasn't authorized yet.
    // Config is saved in completeOAuthLogin after successful code exchange.
    let pending = PendingLoginState {
        pds_url,
        did,
        handle: handle_str,
        dpop_key,
        pkce_verifier: pkce.verifier,
        csrf_state,
        token_endpoint: asm.token_endpoint,
        client_id,
        dpop_nonce,
    };

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct StartResult {
        auth_url: String,
        pending: PendingLoginState,
    }

    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    StartResult { auth_url, pending }
        .serialize(&serializer)
        .map_err(|e| JsError::new(&e.to_string()))
}

// ---------------------------------------------------------------------------
// Complete OAuth login
// ---------------------------------------------------------------------------

/// Complete an OAuth login flow after the user returns from authorization.
///
/// Validates the CSRF state, exchanges the code for tokens (with DPoP),
/// builds the session, and saves it to storage. Tokens never cross the
/// WASM/JS boundary.
#[wasm_bindgen(js_name = completeOAuthLogin)]
pub async fn complete_oauth_login(
    code: &str,
    state: &str,
    pending_js: JsValue,
    redirect_uri: &str,
    storage_adapter: JsStorageAdapter,
) -> Result<(), JsError> {
    let pending: PendingLoginState =
        serde_wasm_bindgen::from_value(pending_js).map_err(|e| JsError::new(&e.to_string()))?;
    let storage = JsStorage::new(storage_adapter);
    let transport = WasmTransport::new();
    let mut rng = OsRng;

    // 1. CSRF validation
    if state != pending.csrf_state {
        return Err(JsError::new("CSRF state mismatch — possible replay attack"));
    }

    // 2. Exchange code for tokens (DPoP-bound, nonce-retried)
    let mut dpop_nonce = pending.dpop_nonce;
    let timestamp = opake_core::client::time::unix_now();

    let token_response = oauth_token::exchange_code(
        &transport,
        &pending.token_endpoint,
        &pending.client_id,
        code,
        redirect_uri,
        &pending.pkce_verifier,
        &pending.dpop_key,
        &mut dpop_nonce,
        Some(&pending.did),
        timestamp,
        &mut rng,
    )
    .await
    .map_err(wasm_err)?;

    // 3. Build session (tokens stay in WASM)
    let now = opake_core::client::time::unix_now();
    let session = OAuthSession {
        did: token_response.sub.unwrap_or_else(|| pending.did.clone()),
        handle: pending.handle.clone(),
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token.unwrap_or_default(),
        dpop_key: pending.dpop_key,
        token_endpoint: pending.token_endpoint,
        dpop_nonce,
        expires_at: token_response.expires_in.map(|e| now + e as i64),
        client_id: pending.client_id,
    };

    // 4. Save session
    let did = session.did.clone();
    storage
        .save_session(&did, &Session::OAuth(session))
        .await
        .map_err(wasm_err)?;

    // 5. Save config
    save_account_config(&storage, &did, &pending.pds_url, &pending.handle).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// App password login
// ---------------------------------------------------------------------------

/// Login with an app password (legacy createSession).
///
/// Resolves the handle, authenticates via the PDS, and saves the session.
/// Tokens never cross the WASM/JS boundary.
#[wasm_bindgen(js_name = loginWithAppPasswordWasm)]
pub async fn login_with_app_password_wasm(
    handle: &str,
    app_password: &str,
    storage_adapter: JsStorageAdapter,
) -> Result<(), JsError> {
    let transport = WasmTransport::new();
    let storage = JsStorage::new(storage_adapter);

    // 1. Resolve handle → PDS URL
    let (_did, pds_url, resolved_handle) = resolve_pds_for_login(&transport, handle)
        .await
        .map_err(wasm_err)?;
    let handle_str = resolved_handle.unwrap_or_else(|| handle.to_string());

    // 2. Login via createSession (all token handling in WASM)
    let mut client = XrpcClient::new(WasmTransport::new(), pds_url.clone());
    client.login(handle, app_password).await.map_err(wasm_err)?;

    // 3. Save session (use the DID from the session response, not from resolution)
    let session = client
        .session()
        .cloned()
        .ok_or_else(|| JsError::new("login produced no session"))?;
    storage
        .save_session(session.did(), &session)
        .await
        .map_err(wasm_err)?;

    // 4. Save config
    save_account_config(&storage, session.did(), &pds_url, &handle_str).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn save_account_config(
    storage: &JsStorage,
    did: &str,
    pds_url: &str,
    handle: &str,
) -> Result<(), JsError> {
    let mut config = storage.load_config().await.unwrap_or_default();

    config.add_account(
        did.to_string(),
        AccountEntry {
            pds_url: pds_url.to_string(),
            handle: handle.to_string(),
        },
    );
    // Login always sets the logged-in account as default.
    config.default_did = Some(did.to_string());

    storage.save_config(&config).await.map_err(wasm_err)
}
