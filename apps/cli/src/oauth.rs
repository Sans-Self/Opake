// OAuth 2.0 login flow for native CLI apps.
//
// Loopback redirect: start a local HTTP server on 127.0.0.1, open the
// browser to the authorization URL, wait for the callback with the auth
// code, exchange it for tokens.

use anyhow::Result;
use chrono::Utc;
use log::{debug, info};
use opake_core::client::dpop::DpopKeyPair;
use opake_core::client::oauth_discovery::{discover_authorization_server, generate_pkce};
use opake_core::client::oauth_token::{
    build_authorization_url, exchange_code, pushed_authorization_request,
};
use opake_core::client::{OAuthSession, Session, XrpcClient};
use opake_core::crypto::OsRng;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::commands::login::ensure_identity_and_publish;
use crate::config::{AccountEntry, FileStorage};
use opake_core::client::ReqwestTransport;

/// Attempt a full OAuth login flow. Returns `Err` if the PDS doesn't support
/// OAuth discovery, so the caller can fall back to password auth.
///
/// `handle` is the resolved handle from the DID document — may differ from
/// `identifier` when the user logs in by DID.
///
/// `no_redirect` controls the callback response: if false (default), redirects
/// to the frontend callback page; if true, serves inline HTML.
pub async fn try_oauth_login(
    pds_url: &str,
    identifier: &str,
    handle: Option<&str>,
    storage: &FileStorage,
    no_redirect: bool,
    force: bool,
) -> Result<Session> {
    let transport = ReqwestTransport::new();

    // Step 1: Discover the authorization server
    info!("attempting OAuth discovery on {pds_url}");
    let (_prm, asm) = discover_authorization_server(&transport, pds_url).await?;
    info!(
        "discovered AS: {} (issuer: {})",
        asm.authorization_endpoint, asm.issuer
    );

    // Step 2: Bind loopback server to get the redirect URI
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    debug!("loopback server on port {port}");

    // Step 3: Generate DPoP keypair and PKCE challenge
    let dpop_key = DpopKeyPair::generate(&mut OsRng);
    let pkce = generate_pkce(&mut OsRng);

    // State parameter for CSRF protection
    let mut state_bytes = [0u8; 16];
    OsRng.fill_bytes(&mut state_bytes);
    let state = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(state_bytes);

    // Client ID: for loopback apps, metadata is encoded in the URL query params.
    // Must be http://localhost (not 127.0.0.1) — the AS recognizes this as a
    // loopback client and uses hardcoded metadata instead of fetching it.
    let scope = opake_core::scope::oauth_scope();
    let client_id = opake_core::client::oauth_token::build_client_id(&redirect_uri, &scope);

    let par_endpoint = asm.par_endpoint();
    debug!("PAR endpoint: {par_endpoint}");

    let mut dpop_nonce = None;
    let timestamp = Utc::now().timestamp();

    // Step 4: Pushed Authorization Request
    let login_hint = handle.or(Some(identifier));
    let par_response = pushed_authorization_request(
        &transport,
        par_endpoint,
        &client_id,
        &redirect_uri,
        &pkce,
        &scope,
        &state,
        login_hint,
        &dpop_key,
        &mut dpop_nonce,
        timestamp,
        &mut OsRng,
    )
    .await?;
    debug!("PAR request_uri: {}", par_response.request_uri);

    // Step 5: Open browser
    let auth_url = build_authorization_url(
        &asm.authorization_endpoint,
        &client_id,
        &par_response.request_uri,
    );
    println!("Opening browser for authentication...");
    println!("If the browser doesn't open, visit:\n  {auth_url}");
    open_browser(&auth_url);

    // Frontend callback URL - used when redirecting after OAuth.
    // Can be overridden via OPAKE_FRONTEND_URL env var.
    let frontend_callback_url = if no_redirect {
        None
    } else {
        let prefix =
            std::env::var("OPAKE_FRONTEND_URL").unwrap_or_else(|_| "https://opake.app".to_string());

        Some(format!("{}/devices/cli-callback", prefix))
    };

    // Step 6: Wait for the callback (PAR request_uri expires)
    let (code, callback_state, error) = wait_for_callback(
        listener,
        par_response.expires_in,
        frontend_callback_url.as_deref(),
    )
    .await?;

    let callback_state =
        callback_state.ok_or_else(|| anyhow::anyhow!("callback missing state parameter"))?;
    anyhow::ensure!(
        callback_state == state,
        "OAuth state mismatch — possible CSRF attack"
    );

    // Check for AS errors (user denied, etc.) after CSRF validation
    if let Some(err) = error {
        anyhow::bail!("OAuth error from AS: {err}");
    }

    let code = code.ok_or_else(|| anyhow::anyhow!("callback missing authorization code"))?;
    info!("received authorization code");

    // Step 7: Exchange code for tokens
    info!("exchanging authorization code for tokens");
    let timestamp = Utc::now().timestamp();
    let token_response = exchange_code(
        &transport,
        &asm.token_endpoint,
        &client_id,
        &code,
        &redirect_uri,
        &pkce.verifier,
        &dpop_key,
        &mut dpop_nonce,
        None, // don't verify sub yet — we'll get DID from the token
        timestamp,
        &mut OsRng,
    )
    .await?;

    info!("token exchange successful");

    let did = token_response
        .sub
        .ok_or_else(|| anyhow::anyhow!("token response missing `sub` claim"))?;
    info!("authenticated as {did}");

    let handle = handle
        .map(|h| h.to_string())
        .or_else(|| {
            // Only use the raw identifier as handle if it's not a DID.
            (!identifier.starts_with("did:")).then(|| identifier.to_string())
        })
        .unwrap_or_default();

    let expires_at = token_response
        .expires_in
        .map(|secs| timestamp + secs as i64);

    let oauth_session = OAuthSession {
        did: did.clone(),
        handle: handle.clone(),
        access_token: token_response.access_token,
        refresh_token: token_response
            .refresh_token
            .ok_or_else(|| anyhow::anyhow!("token response missing refresh_token"))?,
        dpop_key,
        token_endpoint: asm.token_endpoint.clone(),
        dpop_nonce,
        expires_at,
        client_id,
    };

    let session = Session::OAuth(oauth_session);

    // Step 8: Save account config
    let mut cfg = storage.load_config_anyhow().unwrap_or_default();
    cfg.add_account(
        did.clone(),
        AccountEntry {
            pds_url: pds_url.to_string(),
            handle: handle.clone(),
        },
    );
    storage.save_config_anyhow(&cfg)?;

    // Step 9: Identity keypair + public key publication (same as legacy)
    let mut client = XrpcClient::with_session(
        ReqwestTransport::new(),
        pds_url.to_string(),
        session.clone(),
    );

    ensure_identity_and_publish(&mut client, storage, &did, force).await?;
    println!("Logged in as {handle} (OAuth)");

    // If publish_public_key triggered a token refresh (e.g., access token
    // expired during a slow network call or interactive seed-phrase prompt),
    // the client now holds the refreshed session — but our local `session`
    // variable still has the original tokens. Returning the original would
    // persist a stale refresh_token to disk while the AS has already rotated
    // it, leaving the next CLI command unable to refresh. Always return the
    // client's current session so the rotation reaches storage.
    let final_session = client
        .session()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("XRPC client lost its session during login"))?;

    Ok(final_session)
}

/// Wait for the OAuth callback on the loopback server.
/// Returns `(code, state, error)` from the query parameters.
/// Times out after `expires_in` seconds (the PAR request_uri lifetime).
///
/// If `frontend_callback_url` is `Some`, redirects to that URL after OAuth.
/// If `None`, serves inline HTML instead (used with `--no-redirect`).
async fn wait_for_callback(
    listener: TcpListener,
    expires_in: u64,
    frontend_callback_url: Option<&str>,
) -> Result<(Option<String>, Option<String>, Option<String>)> {
    let timeout = std::time::Duration::from_secs(expires_in);
    let (mut stream, _addr) = tokio::time::timeout(timeout, listener.accept())
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "OAuth authorization timed out after {expires_in}s — the request expired"
            )
        })??;

    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await?;
    let request_str = String::from_utf8_lossy(&buf[..n]);

    // Parse GET /callback?code=...&state=... HTTP/1.1
    let path = request_str
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| anyhow::anyhow!("malformed callback request"))?;

    let query = path
        .split_once('?')
        .map(|(_, q)| q)
        .ok_or_else(|| anyhow::anyhow!("callback missing query params"))?;

    let mut code = None;
    let mut state = None;
    let mut error = None;

    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = urlencoding::decode(value)?.into_owned();
        match key {
            "code" => code = Some(value),
            "state" => state = Some(value),
            "error" => error = Some(value),
            _ => {}
        }
    }

    // Build response: redirect to frontend or inline HTML
    let response = if let Some(callback_url) = frontend_callback_url {
        // Redirect to frontend callback — no OAuth params, they're already
        // handled by CLI. Only pass error for display if present.
        let location = if let Some(ref err) = error {
            let err = urlencoding::encode(err);
            format!("{callback_url}?error={err}")
        } else {
            callback_url.to_string()
        };
        format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nConnection: close\r\n\r\n",)
    } else {
        // Inline HTML response (--no-redirect)
        let (status, body) = if error.is_some() {
            ("400 Bad Request", "<html><body><h1>Authentication failed</h1><p>You can close this tab.</p></body></html>")
        } else {
            ("200 OK", "<html><body><h1>Authentication successful</h1><p>You can close this tab and return to your terminal.</p></body></html>")
        };
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    };

    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;

    Ok((code, state, error))
}

/// Open a URL in the system browser. Best-effort — doesn't fail if the
/// browser can't be opened (the URL is printed to stdout as a fallback).
fn open_browser(url: &str) {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };

    if let Err(e) = result {
        debug!("failed to open browser: {e}");
    }
}

use base64::Engine;
use opake_core::crypto::RngCore;
