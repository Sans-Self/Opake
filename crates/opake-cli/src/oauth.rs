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

use crate::config::{AccountConfig, FileStorage};
use crate::identity;
use crate::transport::ReqwestTransport;

/// Attempt a full OAuth login flow. Returns `Err` if the PDS doesn't support
/// OAuth discovery, so the caller can fall back to password auth.
pub async fn try_oauth_login(
    pds_url: &str,
    identifier: &str,
    storage: &FileStorage,
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

    // Client ID: for native apps, use the redirect URI as client_id per atproto spec
    let client_id = format!(
        "http://localhost?redirect_uri={}",
        urlencoding::encode(&redirect_uri)
    );

    let par_endpoint = asm
        .pushed_authorization_request_endpoint
        .as_deref()
        .unwrap_or(&asm.token_endpoint);
    debug!("PAR endpoint: {par_endpoint}");

    let mut dpop_nonce = None;
    let timestamp = Utc::now().timestamp();

    // Step 4: Pushed Authorization Request
    let par_response = pushed_authorization_request(
        &transport,
        par_endpoint,
        &client_id,
        &redirect_uri,
        &pkce,
        "atproto",
        &state,
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

    // Step 6: Wait for the callback (PAR request_uri expires)
    let (code, callback_state) = wait_for_callback(listener, par_response.expires_in).await?;
    anyhow::ensure!(
        callback_state == state,
        "OAuth state mismatch — possible CSRF attack"
    );
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

    let handle = identifier.to_string();

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
        AccountConfig {
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

    let (identity, generated) = identity::ensure_identity(storage, &did, &mut OsRng)?;
    if generated {
        println!("Generated new encryption keypair");
    }

    let public_key_bytes = identity.public_key_bytes()?;
    let verify_key_bytes = identity.verify_key_bytes()?;
    opake_core::resolve::publish_public_key(
        &mut client,
        &public_key_bytes,
        verify_key_bytes.as_ref(),
        &Utc::now().to_rfc3339(),
    )
    .await?;
    println!("Published encryption public key");

    println!("Logged in as {handle} (OAuth)");

    Ok(session)
}

/// Wait for the OAuth callback on the loopback server.
/// Returns `(code, state)` from the query parameters.
/// Times out after `expires_in` seconds (the PAR request_uri lifetime).
async fn wait_for_callback(listener: TcpListener, expires_in: u64) -> Result<(String, String)> {
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

    // Respond to browser
    let (status, body) = if error.is_some() {
        ("400 Bad Request", "<html><body><h1>Authentication failed</h1><p>You can close this tab.</p></body></html>")
    } else {
        ("200 OK", "<html><body><h1>Authentication successful</h1><p>You can close this tab and return to the terminal.</p></body></html>")
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;

    if let Some(err) = error {
        anyhow::bail!("OAuth error from AS: {err}");
    }

    Ok((
        code.ok_or_else(|| anyhow::anyhow!("callback missing `code` parameter"))?,
        state.ok_or_else(|| anyhow::anyhow!("callback missing `state` parameter"))?,
    ))
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
