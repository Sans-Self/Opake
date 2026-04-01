// OAuth 2.0 token operations for AT Protocol.
//
// Pushed Authorization Requests (PAR), authorization code exchange, and token
// refresh — all with DPoP proof attachment and `use_dpop_nonce` retry.
#![allow(clippy::too_many_arguments)]

use log::info;
use serde::Deserialize;

use crate::crypto::{CryptoRng, RngCore};
use crate::error::Error;

use super::dpop::{create_dpop_proof, extract_dpop_nonce, is_use_dpop_nonce_error, DpopKeyPair};
use super::oauth_discovery::PkceChallenge;
use super::transport::{HttpMethod, HttpRequest, HttpResponse, RequestBody, Transport};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Response from a Pushed Authorization Request (RFC 9126).
#[derive(Debug, Deserialize)]
pub struct ParResponse {
    pub request_uri: String,
    pub expires_in: u64,
}

/// Token response from the authorization server.
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
    pub sub: Option<String>,
}

// ---------------------------------------------------------------------------
// PAR
// ---------------------------------------------------------------------------

/// Send a Pushed Authorization Request. Returns the `request_uri` to embed
/// in the browser authorization URL.
///
/// `dpop_nonce` is updated in-place if the AS provides one.
pub async fn pushed_authorization_request(
    transport: &impl Transport,
    par_endpoint: &str,
    client_id: &str,
    redirect_uri: &str,
    pkce: &PkceChallenge,
    scope: &str,
    state: &str,
    dpop_key: &DpopKeyPair,
    dpop_nonce: &mut Option<String>,
    timestamp: i64,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<ParResponse, Error> {
    let params = vec![
        ("client_id".into(), client_id.into()),
        ("response_type".into(), "code".into()),
        ("redirect_uri".into(), redirect_uri.into()),
        ("scope".into(), scope.into()),
        ("state".into(), state.into()),
        ("code_challenge".into(), pkce.challenge.clone()),
        ("code_challenge_method".into(), "S256".into()),
    ];

    let response = send_with_dpop_retry(
        transport,
        par_endpoint,
        "POST",
        params,
        dpop_key,
        dpop_nonce,
        None,
        timestamp,
        rng,
    )
    .await?;

    if response.status != 200 && response.status != 201 {
        return Err(token_error(&response, "PAR request failed"));
    }

    serde_json::from_slice(&response.body)
        .map_err(|e| Error::Auth(format!("invalid PAR response: {e}")))
}

/// Build the authorization URL for the browser redirect.
pub fn build_authorization_url(
    authorization_endpoint: &str,
    client_id: &str,
    request_uri: &str,
) -> String {
    format!(
        "{}?client_id={}&request_uri={}",
        authorization_endpoint,
        urlencoding::encode(client_id),
        urlencoding::encode(request_uri),
    )
}

// ---------------------------------------------------------------------------
// Code exchange
// ---------------------------------------------------------------------------

/// Exchange an authorization code for tokens.
///
/// `dpop_nonce` is updated in-place if the AS provides one.
pub async fn exchange_code(
    transport: &impl Transport,
    token_endpoint: &str,
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    pkce_verifier: &str,
    dpop_key: &DpopKeyPair,
    dpop_nonce: &mut Option<String>,
    expected_did: Option<&str>,
    timestamp: i64,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<TokenResponse, Error> {
    let params = vec![
        ("grant_type".into(), "authorization_code".into()),
        ("client_id".into(), client_id.into()),
        ("code".into(), code.into()),
        ("redirect_uri".into(), redirect_uri.into()),
        ("code_verifier".into(), pkce_verifier.into()),
    ];

    let response = send_with_dpop_retry(
        transport,
        token_endpoint,
        "POST",
        params,
        dpop_key,
        dpop_nonce,
        None,
        timestamp,
        rng,
    )
    .await?;

    if response.status != 200 {
        return Err(token_error(&response, "token exchange failed"));
    }

    let token_response: TokenResponse = serde_json::from_slice(&response.body)
        .map_err(|e| Error::Auth(format!("invalid token response: {e}")))?;

    validate_token_response(&token_response, expected_did)?;
    Ok(token_response)
}

// ---------------------------------------------------------------------------
// Refresh
// ---------------------------------------------------------------------------

/// Refresh an access token using a refresh token.
///
/// `dpop_nonce` is updated in-place if the AS provides one.
pub async fn refresh_token(
    transport: &impl Transport,
    token_endpoint: &str,
    client_id: &str,
    refresh_token_value: &str,
    dpop_key: &DpopKeyPair,
    dpop_nonce: &mut Option<String>,
    timestamp: i64,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<TokenResponse, Error> {
    let params = vec![
        ("grant_type".into(), "refresh_token".into()),
        ("client_id".into(), client_id.into()),
        ("refresh_token".into(), refresh_token_value.into()),
    ];

    let response = send_with_dpop_retry(
        transport,
        token_endpoint,
        "POST",
        params,
        dpop_key,
        dpop_nonce,
        None,
        timestamp,
        rng,
    )
    .await?;

    if response.status != 200 {
        return Err(token_error(&response, "token refresh failed"));
    }

    let token_response: TokenResponse = serde_json::from_slice(&response.body)
        .map_err(|e| Error::Auth(format!("invalid token refresh response: {e}")))?;

    validate_token_response(&token_response, None)?;
    Ok(token_response)
}

// ---------------------------------------------------------------------------
// DPoP retry helper
// ---------------------------------------------------------------------------

/// Send a form POST with a DPoP proof, retrying once on `use_dpop_nonce`.
///
/// This implements the server nonce negotiation dance:
/// 1. Send request with DPoP proof (no nonce, or stale nonce)
/// 2. AS responds 400 `use_dpop_nonce` + `DPoP-Nonce` header
/// 3. Re-send with fresh nonce from the header
async fn send_with_dpop_retry(
    transport: &impl Transport,
    url: &str,
    method: &str,
    params: Vec<(String, String)>,
    dpop_key: &DpopKeyPair,
    dpop_nonce: &mut Option<String>,
    access_token: Option<&str>,
    timestamp: i64,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<HttpResponse, Error> {
    let response = send_dpop_request(
        transport,
        url,
        method,
        params.clone(),
        dpop_key,
        dpop_nonce.as_deref(),
        access_token,
        timestamp,
        rng,
    )
    .await?;

    // Capture nonce from every response
    if let Some(nonce) = extract_dpop_nonce(&response) {
        *dpop_nonce = Some(nonce);
    }

    if is_use_dpop_nonce_error(&response) {
        info!("AS requested DPoP nonce, retrying");

        let retry = send_dpop_request(
            transport,
            url,
            method,
            params,
            dpop_key,
            dpop_nonce.as_deref(),
            access_token,
            timestamp,
            rng,
        )
        .await?;

        if let Some(nonce) = extract_dpop_nonce(&retry) {
            *dpop_nonce = Some(nonce);
        }

        return Ok(retry);
    }

    Ok(response)
}

async fn send_dpop_request(
    transport: &impl Transport,
    url: &str,
    method: &str,
    params: Vec<(String, String)>,
    dpop_key: &DpopKeyPair,
    nonce: Option<&str>,
    access_token: Option<&str>,
    timestamp: i64,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<HttpResponse, Error> {
    let proof = create_dpop_proof(dpop_key, method, url, timestamp, nonce, access_token, rng)?;

    let request = HttpRequest {
        method: HttpMethod::Post,
        url: url.to_string(),
        headers: vec![("DPoP".into(), proof)],
        body: Some(RequestBody::Form(params)),
    };

    transport.send(request).await
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn validate_token_response(
    response: &TokenResponse,
    expected_did: Option<&str>,
) -> Result<(), Error> {
    if !response.token_type.eq_ignore_ascii_case("DPoP") {
        return Err(Error::Auth(format!(
            "expected token_type \"DPoP\", got \"{}\"",
            response.token_type
        )));
    }

    if let Some(scope) = &response.scope {
        if !scope.split_whitespace().any(|s| s == "atproto") {
            return Err(Error::Auth(format!(
                "token scope missing \"atproto\": \"{scope}\""
            )));
        }
    }

    if let Some(did) = expected_did {
        if let Some(sub) = &response.sub {
            if sub != did {
                return Err(Error::Auth(format!(
                    "token sub \"{sub}\" does not match expected DID \"{did}\""
                )));
            }
        }
    }

    Ok(())
}

fn token_error(response: &HttpResponse, context: &str) -> Error {
    #[derive(Deserialize)]
    struct OAuthError {
        error: Option<String>,
        error_description: Option<String>,
    }

    let detail = serde_json::from_slice::<OAuthError>(&response.body)
        .ok()
        .and_then(|e| match (e.error, e.error_description) {
            (Some(code), Some(desc)) => Some(format!("{code}: {desc}")),
            (Some(code), None) => Some(code),
            _ => None,
        })
        .unwrap_or_else(|| format!("HTTP {}", response.status));

    Error::Auth(format!("{context}: {detail}"))
}

#[cfg(test)]
#[path = "oauth_token_tests.rs"]
mod tests;
