use super::*;
use crate::crypto::OsRng;

#[test]
fn generate_keypair_produces_valid_jwk() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    assert_eq!(kp.public_jwk.kty, "EC");
    assert_eq!(kp.public_jwk.crv, "P-256");
    assert!(!kp.public_jwk.x.is_empty());
    assert!(!kp.public_jwk.y.is_empty());
}

#[test]
fn keypair_roundtrips_through_json() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    let json = serde_json::to_string(&kp).unwrap();
    let restored: DpopKeyPair = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.public_jwk, kp.public_jwk);
}

// spec:wasm-security-boundary § Token-bearing types zeroize and redact on the WASM side
#[test]
fn keypair_debug_redacts_private_key() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    let debug = format!("{kp:?}");
    // RedactedDebug renders String fields as "[N bytes]"
    assert!(
        debug.contains("bytes]"),
        "expected redacted output, got: {debug}"
    );
    assert!(!debug.contains(&kp.private_key_b64));
}

#[test]
fn create_proof_produces_three_part_jwt() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    let proof = create_dpop_proof(
        &kp,
        "POST",
        "https://pds.example/token",
        1700000000,
        None,
        None,
        &mut OsRng,
    )
    .unwrap();
    let parts: Vec<&str> = proof.split('.').collect();
    assert_eq!(parts.len(), 3, "JWT must have 3 parts: {proof}");
}

#[test]
fn proof_header_has_correct_fields() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    let proof = create_dpop_proof(
        &kp,
        "POST",
        "https://pds.example/token",
        1700000000,
        None,
        None,
        &mut OsRng,
    )
    .unwrap();
    let header_b64 = proof.split('.').next().unwrap();
    let header: serde_json::Value =
        serde_json::from_slice(&BASE64URL.decode(header_b64).unwrap()).unwrap();
    assert_eq!(header["typ"], "dpop+jwt");
    assert_eq!(header["alg"], "ES256");
    assert_eq!(header["jwk"]["kty"], "EC");
    assert_eq!(header["jwk"]["crv"], "P-256");
}

#[test]
fn proof_payload_has_required_claims() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    let proof = create_dpop_proof(
        &kp,
        "GET",
        "https://pds.example/xrpc/foo",
        1700000000,
        None,
        None,
        &mut OsRng,
    )
    .unwrap();
    let payload_b64 = proof.split('.').nth(1).unwrap();
    let payload: serde_json::Value =
        serde_json::from_slice(&BASE64URL.decode(payload_b64).unwrap()).unwrap();
    assert_eq!(payload["htm"], "GET");
    assert_eq!(payload["htu"], "https://pds.example/xrpc/foo");
    assert_eq!(payload["iat"], 1700000000);
    assert!(payload["jti"].is_string());
    assert!(payload.get("nonce").is_none());
    assert!(payload.get("ath").is_none());
}

#[test]
fn proof_includes_nonce_when_provided() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    let proof = create_dpop_proof(
        &kp,
        "POST",
        "https://pds.example/token",
        1700000000,
        Some("server-nonce-42"),
        None,
        &mut OsRng,
    )
    .unwrap();
    let payload_b64 = proof.split('.').nth(1).unwrap();
    let payload: serde_json::Value =
        serde_json::from_slice(&BASE64URL.decode(payload_b64).unwrap()).unwrap();
    assert_eq!(payload["nonce"], "server-nonce-42");
}

#[test]
fn proof_includes_ath_when_access_token_provided() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    let proof = create_dpop_proof(
        &kp,
        "GET",
        "https://pds.example/xrpc/foo",
        1700000000,
        None,
        Some("my-access-token"),
        &mut OsRng,
    )
    .unwrap();
    let payload_b64 = proof.split('.').nth(1).unwrap();
    let payload: serde_json::Value =
        serde_json::from_slice(&BASE64URL.decode(payload_b64).unwrap()).unwrap();
    // ath = base64url(sha256(access_token))
    let expected_hash = Sha256::digest(b"my-access-token");
    let expected_ath = BASE64URL.encode(expected_hash);
    assert_eq!(payload["ath"], expected_ath);
}

#[test]
fn proof_signature_is_64_bytes_raw() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    let proof = create_dpop_proof(
        &kp,
        "POST",
        "https://pds.example/token",
        1700000000,
        None,
        None,
        &mut OsRng,
    )
    .unwrap();
    let sig_b64 = proof.split('.').nth(2).unwrap();
    let sig_bytes = BASE64URL.decode(sig_b64).unwrap();
    assert_eq!(
        sig_bytes.len(),
        64,
        "ES256 raw r‖s signature must be 64 bytes"
    );
}

#[test]
fn proof_signature_verifies() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    let proof = create_dpop_proof(
        &kp,
        "POST",
        "https://pds.example/token",
        1700000000,
        None,
        None,
        &mut OsRng,
    )
    .unwrap();
    let parts: Vec<&str> = proof.split('.').collect();
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let sig_bytes = BASE64URL.decode(parts[2]).unwrap();
    let signature = Signature::from_bytes(sig_bytes.as_slice().into()).unwrap();
    let verifying_key = VerifyingKey::from(&kp.signing_key().unwrap());
    use p256::ecdsa::signature::Verifier;
    verifying_key
        .verify(signing_input.as_bytes(), &signature)
        .unwrap();
}

#[test]
fn jti_is_unique_per_proof() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    let extract_jti = |proof: &str| -> String {
        let payload_b64 = proof.split('.').nth(1).unwrap();
        let payload: serde_json::Value =
            serde_json::from_slice(&BASE64URL.decode(payload_b64).unwrap()).unwrap();
        payload["jti"].as_str().unwrap().to_string()
    };
    let p1 = create_dpop_proof(&kp, "POST", "https://x", 1, None, None, &mut OsRng).unwrap();
    let p2 = create_dpop_proof(&kp, "POST", "https://x", 1, None, None, &mut OsRng).unwrap();
    assert_ne!(extract_jti(&p1), extract_jti(&p2));
}

#[test]
fn jwk_thumbprint_is_deterministic() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    assert_eq!(kp.jwk_thumbprint(), kp.jwk_thumbprint());
}

#[test]
fn jwk_thumbprint_differs_between_keys() {
    let kp1 = DpopKeyPair::generate(&mut OsRng);
    let kp2 = DpopKeyPair::generate(&mut OsRng);
    assert_ne!(kp1.jwk_thumbprint(), kp2.jwk_thumbprint());
}

// -- nonce helpers --

#[test]
fn extract_dpop_nonce_finds_header() {
    let response = HttpResponse {
        status: 200,
        headers: vec![("DPoP-Nonce".into(), "abc123".into())],
        body: vec![],
    };
    assert_eq!(extract_dpop_nonce(&response).unwrap(), "abc123");
}

#[test]
fn extract_dpop_nonce_case_insensitive() {
    let response = HttpResponse {
        status: 200,
        headers: vec![("dpop-nonce".into(), "lower".into())],
        body: vec![],
    };
    assert_eq!(extract_dpop_nonce(&response).unwrap(), "lower");
}

#[test]
fn extract_dpop_nonce_missing_returns_none() {
    let response = HttpResponse {
        status: 200,
        headers: vec![],
        body: vec![],
    };
    assert!(extract_dpop_nonce(&response).is_none());
}

#[test]
fn is_use_dpop_nonce_error_detects_correctly() {
    let response = HttpResponse {
        status: 400,
        headers: vec![("DPoP-Nonce".into(), "new-nonce".into())],
        body: br#"{"error":"use_dpop_nonce"}"#.to_vec(),
    };
    assert!(is_use_dpop_nonce_error(&response));
}

#[test]
fn is_use_dpop_nonce_error_rejects_other_400() {
    let response = HttpResponse {
        status: 400,
        headers: vec![],
        body: br#"{"error":"invalid_request"}"#.to_vec(),
    };
    assert!(!is_use_dpop_nonce_error(&response));
}

#[test]
fn is_use_dpop_nonce_error_detects_401() {
    let response = HttpResponse {
        status: 401,
        headers: vec![("DPoP-Nonce".into(), "pds-nonce".into())],
        body: br#"{"error":"use_dpop_nonce"}"#.to_vec(),
    };
    assert!(is_use_dpop_nonce_error(&response));
}

#[test]
fn is_use_dpop_nonce_error_rejects_other_status() {
    let response = HttpResponse {
        status: 403,
        headers: vec![],
        body: br#"{"error":"use_dpop_nonce"}"#.to_vec(),
    };
    assert!(!is_use_dpop_nonce_error(&response));
}

// -- htu stripping --

#[test]
fn htu_strips_query_string() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    let proof = create_dpop_proof(
        &kp,
        "GET",
        "https://pds.example/xrpc/foo?bar=1",
        1700000000,
        None,
        None,
        &mut OsRng,
    )
    .unwrap();
    let payload_b64 = proof.split('.').nth(1).unwrap();
    let payload: serde_json::Value =
        serde_json::from_slice(&BASE64URL.decode(payload_b64).unwrap()).unwrap();
    assert_eq!(payload["htu"], "https://pds.example/xrpc/foo");
}

#[test]
fn htu_strips_fragment() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    let proof = create_dpop_proof(
        &kp,
        "GET",
        "https://pds.example/xrpc/foo#section",
        1700000000,
        None,
        None,
        &mut OsRng,
    )
    .unwrap();
    let payload_b64 = proof.split('.').nth(1).unwrap();
    let payload: serde_json::Value =
        serde_json::from_slice(&BASE64URL.decode(payload_b64).unwrap()).unwrap();
    assert_eq!(payload["htu"], "https://pds.example/xrpc/foo");
}

#[test]
fn htu_strips_query_and_fragment() {
    let kp = DpopKeyPair::generate(&mut OsRng);
    let proof = create_dpop_proof(
        &kp,
        "GET",
        "https://pds.example/path?q=1#frag",
        1700000000,
        None,
        None,
        &mut OsRng,
    )
    .unwrap();
    let payload_b64 = proof.split('.').nth(1).unwrap();
    let payload: serde_json::Value =
        serde_json::from_slice(&BASE64URL.decode(payload_b64).unwrap()).unwrap();
    assert_eq!(payload["htu"], "https://pds.example/path");
}

#[test]
fn strip_query_fragment_no_op_for_clean_url() {
    assert_eq!(
        strip_query_fragment("https://example.com/path"),
        "https://example.com/path"
    );
}
