// DPoP (Demonstrating Proof-of-Possession) for OAuth 2.0.
//
// Hand-rolled ES256 JWT proofs. We only *create* DPoP proofs, never verify
// them, so pulling in a full JWT crate would be overkill (and a dependency
// nightmare for WASM). The JWS signature is raw r‖s (64 bytes), not DER.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as BASE64URL, Engine};
use p256::ecdsa::{signature::Signer, Signature, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::crypto::{CryptoRng, RngCore};
use crate::error::Error;

use super::transport::HttpResponse;

// ---------------------------------------------------------------------------
// DPoP keypair
// ---------------------------------------------------------------------------

/// A P-256 keypair used for DPoP proof generation. Session-scoped, not
/// identity-scoped — created fresh on each OAuth login.
#[derive(Clone, Serialize, Deserialize)]
pub struct DpopKeyPair {
    /// SEC1-encoded private key bytes (32 bytes), base64url-encoded for storage.
    #[serde(rename = "privateKey")]
    private_key_b64: String,
    /// JWK public key (the `x` and `y` coordinates). Embedded directly in
    /// every DPoP proof header.
    #[serde(rename = "publicJwk")]
    public_jwk: DpopPublicJwk,
}

/// The public half of a DPoP key, serialized as a JWK in the DPoP JWT header.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DpopPublicJwk {
    pub kty: String,
    pub crv: String,
    pub x: String,
    pub y: String,
}

impl std::fmt::Debug for DpopKeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DpopKeyPair")
            .field("private_key_b64", &"[redacted]")
            .field("public_jwk", &self.public_jwk)
            .finish()
    }
}

impl DpopKeyPair {
    /// Generate a fresh P-256 keypair for DPoP proofs.
    pub fn generate(rng: &mut (impl CryptoRng + RngCore)) -> Self {
        let signing_key = SigningKey::random(rng);
        let verifying_key = VerifyingKey::from(&signing_key);

        let encoded_point = verifying_key.to_encoded_point(false);
        let public_jwk = DpopPublicJwk {
            kty: "EC".into(),
            crv: "P-256".into(),
            x: BASE64URL.encode(encoded_point.x().unwrap()),
            y: BASE64URL.encode(encoded_point.y().unwrap()),
        };

        let private_key_b64 = BASE64URL.encode(signing_key.to_bytes());

        Self {
            private_key_b64,
            public_jwk,
        }
    }

    fn signing_key(&self) -> Result<SigningKey, Error> {
        let bytes = BASE64URL
            .decode(&self.private_key_b64)
            .map_err(|e| Error::Auth(format!("invalid DPoP private key: {e}")))?;
        SigningKey::from_bytes(bytes.as_slice().into())
            .map_err(|e| Error::Auth(format!("invalid DPoP private key: {e}")))
    }

    /// The public JWK for embedding in DPoP proof headers.
    pub fn public_jwk(&self) -> &DpopPublicJwk {
        &self.public_jwk
    }

    /// JWK thumbprint (S256) of the public key, per RFC 7638.
    /// Used as the `jkt` confirmation claim in token introspection.
    pub fn jwk_thumbprint(&self) -> String {
        let canonical = format!(
            r#"{{"crv":"{}","kty":"{}","x":"{}","y":"{}"}}"#,
            self.public_jwk.crv, self.public_jwk.kty, self.public_jwk.x, self.public_jwk.y,
        );
        let hash = Sha256::digest(canonical.as_bytes());
        BASE64URL.encode(hash)
    }
}

// ---------------------------------------------------------------------------
// DPoP proof creation
// ---------------------------------------------------------------------------

/// Create a DPoP proof JWT for a given HTTP method and URL.
///
/// The proof is a compact JWS (header.payload.signature) with:
/// - `typ: "dpop+jwt"`, `alg: "ES256"`, `jwk: <public key>`
/// - `jti: <unique id>`, `htm: <method>`, `htu: <url>`, `iat: <timestamp>`
/// - Optional `nonce` (from AS `DPoP-Nonce` header)
/// - Optional `ath` (access token hash, for resource server requests)
///
/// Timestamp is injected so callers can control time (WASM has no clock).
/// RNG is injected for the `jti` claim.
pub fn create_dpop_proof(
    keypair: &DpopKeyPair,
    method: &str,
    url: &str,
    timestamp: i64,
    nonce: Option<&str>,
    access_token: Option<&str>,
    rng: &mut (impl CryptoRng + RngCore),
) -> Result<String, Error> {
    let signing_key = keypair.signing_key()?;

    // Header
    let header = serde_json::json!({
        "typ": "dpop+jwt",
        "alg": "ES256",
        "jwk": keypair.public_jwk,
    });

    // jti: random 16 bytes, base64url-encoded
    let mut jti_bytes = [0u8; 16];
    rng.fill_bytes(&mut jti_bytes);
    let jti = BASE64URL.encode(jti_bytes);

    // htu: strip query and fragment per RFC 9449 §4.2
    let htu = strip_query_fragment(url);

    // Payload
    let mut payload = serde_json::json!({
        "jti": jti,
        "htm": method,
        "htu": htu,
        "iat": timestamp,
    });

    if let Some(n) = nonce {
        payload["nonce"] = serde_json::Value::String(n.to_string());
    }

    if let Some(token) = access_token {
        let hash = Sha256::digest(token.as_bytes());
        payload["ath"] = serde_json::Value::String(BASE64URL.encode(hash));
    }

    // Encode
    let header_b64 = BASE64URL.encode(serde_json::to_vec(&header).unwrap());
    let payload_b64 = BASE64URL.encode(serde_json::to_vec(&payload).unwrap());
    let signing_input = format!("{header_b64}.{payload_b64}");

    // Sign — raw r‖s (64 bytes), NOT DER
    let signature: Signature = signing_key.sign(signing_input.as_bytes());
    let sig_b64 = BASE64URL.encode(signature.to_bytes());

    Ok(format!("{signing_input}.{sig_b64}"))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip query string and fragment from a URL (RFC 9449 §4.2).
fn strip_query_fragment(url: &str) -> &str {
    let end = url.find('?').or_else(|| url.find('#')).unwrap_or(url.len());
    &url[..end]
}

// ---------------------------------------------------------------------------
// DPoP nonce helpers
// ---------------------------------------------------------------------------

/// Extract the `DPoP-Nonce` header from an HTTP response.
pub fn extract_dpop_nonce(response: &HttpResponse) -> Option<String> {
    response.header("dpop-nonce").map(|v| v.to_string())
}

/// Check whether a response is a `use_dpop_nonce` error — the AS telling us
/// to retry with the nonce it provided in the `DPoP-Nonce` header.
pub fn is_use_dpop_nonce_error(response: &HttpResponse) -> bool {
    if response.status != 400 {
        return false;
    }

    #[derive(Deserialize)]
    struct ErrorBody {
        error: Option<String>,
    }

    serde_json::from_slice::<ErrorBody>(&response.body)
        .ok()
        .and_then(|b| b.error)
        .is_some_and(|e| e == "use_dpop_nonce")
}

#[cfg(test)]
#[path = "dpop_tests.rs"]
mod tests;
