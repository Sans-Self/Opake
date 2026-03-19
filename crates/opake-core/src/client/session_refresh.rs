// Standalone proactive session refresh.
//
// Decoupled from XrpcClient so the CLI daemon and web Service Worker
// can refresh sessions without constructing a full XRPC client.

use log::info;

use super::oauth_token;
use super::transport::*;
use super::xrpc::{LegacySession, OAuthSession, Session};
use crate::crypto::{CryptoRng, RngCore};
use crate::error::Error;

/// Result of a proactive refresh attempt.
#[derive(Debug)]
pub enum RefreshOutcome {
    /// Session was refreshed. Contains the updated session.
    Refreshed(Box<Session>),
    /// Session does not need refreshing yet.
    NotNeeded,
    /// Refresh failed. The original session is still usable until it
    /// actually expires. Contains the error for logging.
    Failed(Error),
}

/// Default threshold: refresh if expiring within 60 seconds.
/// Must be shorter than the AS token lifetime (typically 5 minutes for
/// atproto) to avoid triggering a refresh on every single check.
pub const DEFAULT_REFRESH_THRESHOLD_SECONDS: i64 = 60;

/// Check whether a session needs refreshing and, if so, refresh it.
///
/// This is the single implementation shared by CLI daemon and web SW.
/// It does NOT persist the session — the caller is responsible for that.
///
/// `pds_url` is needed for legacy sessions (to construct the refreshSession
/// endpoint URL). OAuth sessions carry their own `token_endpoint`.
pub async fn proactive_refresh(
    transport: &impl Transport,
    session: &Session,
    pds_url: &str,
    threshold_seconds: i64,
    now: i64,
    rng: &mut (impl CryptoRng + RngCore),
) -> RefreshOutcome {
    if !session.needs_refresh(threshold_seconds, now) {
        return RefreshOutcome::NotNeeded;
    }

    match session {
        Session::Legacy(s) => refresh_legacy(transport, s, pds_url).await,
        Session::OAuth(s) => refresh_oauth(transport, s, now, rng).await,
    }
}

// Duplicates the HTTP call in `XrpcClient::refresh_legacy` (auth.rs).
// The XrpcClient version mutates internal state (`self.session`, `self.session_refreshed`),
// so it can't be extracted without a larger refactor of the client ownership model.
// This standalone version returns a value instead, for use by the daemon and SW
// which don't own an XrpcClient.
async fn refresh_legacy(
    transport: &impl Transport,
    session: &LegacySession,
    pds_url: &str,
) -> RefreshOutcome {
    info!("proactive refresh: legacy session for {}", session.handle);

    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Post,
            url: format!("{pds_url}/xrpc/com.atproto.server.refreshSession"),
            headers: vec![(
                "Authorization".into(),
                format!("Bearer {}", session.refresh_jwt),
            )],
            body: None,
        })
        .await;

    let response = match response {
        Ok(r) => r,
        Err(e) => return RefreshOutcome::Failed(e),
    };

    if response.status != 200 {
        return RefreshOutcome::Failed(Error::Auth(format!(
            "legacy session refresh failed (HTTP {})",
            response.status
        )));
    }

    match serde_json::from_slice::<LegacySession>(&response.body) {
        Ok(new_session) => {
            info!(
                "proactive refresh: legacy session refreshed for {}",
                new_session.handle
            );
            RefreshOutcome::Refreshed(Box::new(Session::Legacy(new_session)))
        }
        Err(e) => {
            RefreshOutcome::Failed(Error::Auth(format!("invalid legacy refresh response: {e}")))
        }
    }
}

async fn refresh_oauth(
    transport: &impl Transport,
    session: &OAuthSession,
    now: i64,
    rng: &mut (impl CryptoRng + RngCore),
) -> RefreshOutcome {
    info!("proactive refresh: OAuth session for {}", session.handle);

    let mut dpop_nonce = session.dpop_nonce.clone();

    let result = oauth_token::refresh_token(
        transport,
        &session.token_endpoint,
        &session.client_id,
        &session.refresh_token,
        &session.dpop_key,
        &mut dpop_nonce,
        now,
        rng,
    )
    .await;

    match result {
        Ok(token_response) => {
            let mut updated = session.clone();
            updated.apply_token_response(&token_response, now);
            updated.dpop_nonce = dpop_nonce;
            info!(
                "proactive refresh: OAuth session refreshed for {}",
                updated.handle
            );
            RefreshOutcome::Refreshed(Box::new(Session::OAuth(updated)))
        }
        Err(e) => RefreshOutcome::Failed(e),
    }
}

#[cfg(test)]
#[path = "session_refresh_tests.rs"]
mod tests;
