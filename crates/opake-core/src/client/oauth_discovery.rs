// OAuth 2.0 discovery for AT Protocol PDSes.
//
// Two-step discovery: first fetch the Protected Resource Metadata from the PDS,
// then fetch the Authorization Server Metadata from the AS it points to.
// Also includes PKCE (S256) generation since it's tightly coupled to the
// OAuth flow and needs no extra deps (sha2 is already in the tree).

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as BASE64URL, Engine};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::crypto::{CryptoRng, RngCore};
use crate::error::Error;

use super::transport::{HttpMethod, HttpRequest, HttpResponse, Transport};

// ---------------------------------------------------------------------------
// Metadata types
// ---------------------------------------------------------------------------

/// OAuth Protected Resource Metadata (RFC 9728).
/// Fetched from `<pds>/.well-known/oauth-protected-resource`.
#[derive(Debug, Clone, Deserialize)]
pub struct ProtectedResourceMetadata {
    /// The PDS resource identifier (its own URL).
    pub resource: String,
    /// Authorization servers that protect this resource.
    pub authorization_servers: Vec<String>,
    /// Token endpoint auth methods the resource supports.
    #[serde(default)]
    pub bearer_methods_supported: Vec<String>,
    /// Scopes available at this resource.
    #[serde(default)]
    pub scopes_supported: Vec<String>,
}

/// OAuth Authorization Server Metadata (RFC 8414).
/// Fetched from `<as>/.well-known/oauth-authorization-server`.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthorizationServerMetadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub pushed_authorization_request_endpoint: Option<String>,
    #[serde(default)]
    pub scopes_supported: Vec<String>,
    #[serde(default)]
    pub response_types_supported: Vec<String>,
    #[serde(default)]
    pub grant_types_supported: Vec<String>,
    #[serde(default)]
    pub code_challenge_methods_supported: Vec<String>,
    #[serde(default)]
    pub dpop_signing_alg_values_supported: Vec<String>,
    #[serde(default)]
    pub token_endpoint_auth_methods_supported: Vec<String>,
    /// Whether this AS requires PAR (RFC 9126).
    #[serde(default)]
    pub require_pushed_authorization_requests: bool,
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Discover the OAuth authorization server for a PDS.
///
/// 1. Fetch `<pds_url>/.well-known/oauth-protected-resource`
/// 2. Extract the first `authorization_servers` entry
/// 3. Fetch `<as_url>/.well-known/oauth-authorization-server`
///
/// Returns both metadata documents. Errors if the PDS doesn't support OAuth
/// (no protected resource metadata) or the AS metadata fetch fails.
pub async fn discover_authorization_server(
    transport: &impl Transport,
    pds_url: &str,
) -> Result<(ProtectedResourceMetadata, AuthorizationServerMetadata), Error> {
    let pds_base = pds_url.trim_end_matches('/');

    // Step 1: Protected Resource Metadata
    let prm_url = format!("{pds_base}/.well-known/oauth-protected-resource");
    let prm_response = fetch_json(transport, &prm_url).await?;
    let prm: ProtectedResourceMetadata = serde_json::from_slice(&prm_response.body)
        .map_err(|e| Error::Auth(format!("invalid protected resource metadata: {e}")))?;

    let as_url = prm.authorization_servers.first().ok_or_else(|| {
        Error::Auth("no authorization servers in protected resource metadata".into())
    })?;

    // Step 2: Authorization Server Metadata
    let as_base = as_url.trim_end_matches('/');
    let asm_url = format!("{as_base}/.well-known/oauth-authorization-server");
    let asm_response = fetch_json(transport, &asm_url).await?;
    let asm: AuthorizationServerMetadata = serde_json::from_slice(&asm_response.body)
        .map_err(|e| Error::Auth(format!("invalid authorization server metadata: {e}")))?;

    Ok((prm, asm))
}

async fn fetch_json(transport: &impl Transport, url: &str) -> Result<HttpResponse, Error> {
    let response = transport
        .send(HttpRequest {
            method: HttpMethod::Get,
            url: url.to_string(),
            headers: vec![("Accept".into(), "application/json".into())],
            body: None,
        })
        .await?;

    if response.status != 200 {
        return Err(Error::Auth(format!(
            "OAuth discovery failed: GET {url} returned HTTP {}",
            response.status
        )));
    }

    Ok(response)
}

// ---------------------------------------------------------------------------
// PAR endpoint resolution
// ---------------------------------------------------------------------------

impl AuthorizationServerMetadata {
    /// The PAR endpoint, falling back to the token endpoint if the AS doesn't
    /// advertise one. The spec requires PAR support — this fallback handles
    /// older/incomplete AS metadata gracefully.
    pub fn par_endpoint(&self) -> &str {
        self.pushed_authorization_request_endpoint
            .as_deref()
            .unwrap_or(&self.token_endpoint)
    }
}

// ---------------------------------------------------------------------------
// PKCE (RFC 7636)
// ---------------------------------------------------------------------------

/// A PKCE code verifier + challenge pair (S256 method).
#[derive(Debug, Clone)]
pub struct PkceChallenge {
    /// The raw verifier string (sent with the token exchange).
    pub verifier: String,
    /// The S256 challenge (sent with the authorization request).
    pub challenge: String,
}

/// Generate a PKCE S256 code challenge from 32 random bytes.
///
/// verifier = base64url(random_bytes)
/// challenge = base64url(sha256(verifier))
pub fn generate_pkce(rng: &mut (impl CryptoRng + RngCore)) -> PkceChallenge {
    let mut bytes = [0u8; 32];
    rng.fill_bytes(&mut bytes);

    let verifier = BASE64URL.encode(bytes);
    let challenge = BASE64URL.encode(Sha256::digest(verifier.as_bytes()));

    PkceChallenge {
        verifier,
        challenge,
    }
}

#[cfg(test)]
#[path = "oauth_discovery_tests.rs"]
mod tests;
