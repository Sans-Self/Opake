// Auth signing for indexer requests.
//
// Pure function — no I/O, no clock. The caller provides the timestamp.
// Produces the `Authorization: Opake-Ed25519 <did>:<ts>:<sig>` header value
// that the indexer's auth middleware expects.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};

/// Build the signed Authorization header value for an indexer request.
///
/// Signature covers: `<method>:<path>:<timestamp>:<did>`
/// Returns: `Opake-Ed25519 <did>:<timestamp>:<base64(signature)>`
pub fn sign_indexer_request(
    method: &str,
    path: &str,
    did: &str,
    signing_key: &[u8; 32],
    timestamp: u64,
) -> String {
    let key = SigningKey::from_bytes(signing_key);
    let message = format!("{method}:{path}:{timestamp}:{did}");
    let signature = key.sign(message.as_bytes());
    let sig_b64 = BASE64.encode(signature.to_bytes());
    format!("Opake-Ed25519 {did}:{timestamp}:{sig_b64}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::VerifyingKey;
    use ed25519_dalek::{Signature, Verifier};

    fn test_keypair() -> ([u8; 32], VerifyingKey) {
        let secret = [42u8; 32];
        let signing = SigningKey::from_bytes(&secret);
        let verifying = signing.verifying_key();
        (secret, verifying)
    }

    #[test]
    fn roundtrip_signature_verifies() {
        let (secret, verifying) = test_keypair();
        let header = sign_indexer_request("GET", "/api/inbox", "did:plc:abc", &secret, 1709330400);

        let payload = header.strip_prefix("Opake-Ed25519 ").unwrap();
        // Parse from right: sig, then timestamp, then did
        let last_colon = payload.rfind(':').unwrap();
        let sig_b64 = &payload[last_colon + 1..];
        let rest = &payload[..last_colon];
        let second_colon = rest.rfind(':').unwrap();
        let timestamp = &rest[second_colon + 1..];
        let did = &rest[..second_colon];

        let sig_bytes = BASE64.decode(sig_b64).unwrap();
        let signature = Signature::from_slice(&sig_bytes).unwrap();
        let message = format!("GET:/api/inbox:{timestamp}:{did}");
        verifying.verify(message.as_bytes(), &signature).unwrap();
    }

    #[test]
    fn header_format_is_correct() {
        let secret = [1u8; 32];
        let header = sign_indexer_request("GET", "/api/inbox", "did:plc:test", &secret, 12345);
        assert!(header.starts_with("Opake-Ed25519 did:plc:test:12345:"));
    }

    #[test]
    fn different_inputs_produce_different_signatures() {
        let secret = [7u8; 32];
        let sig_a = sign_indexer_request("GET", "/api/inbox", "did:plc:a", &secret, 100);
        let sig_b = sign_indexer_request("GET", "/api/inbox", "did:plc:b", &secret, 100);
        let sig_c = sign_indexer_request("POST", "/api/inbox", "did:plc:a", &secret, 100);
        let sig_d = sign_indexer_request("GET", "/api/inbox", "did:plc:a", &secret, 200);

        assert_ne!(sig_a, sig_b);
        assert_ne!(sig_a, sig_c);
        assert_ne!(sig_a, sig_d);
    }
}
