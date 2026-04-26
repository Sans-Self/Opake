use super::*;

#[test]
fn pair_request_new_sets_defaults() {
    let record = PairRequest::new(&[42u8; 32], &[0xABu8; 1184], "2026-03-06T00:00:00Z");
    assert_eq!(record.opake_version, SCHEMA_VERSION);
    assert_eq!(record.algo, PAIR_REQUEST_ALGO);
    assert_eq!(record.created_at, "2026-03-06T00:00:00Z");
}

#[test]
fn pair_request_roundtrips_through_json() {
    let record = PairRequest::new(&[7u8; 32], &[0xCDu8; 1184], "2026-03-06T12:00:00Z");
    let json = serde_json::to_string(&record).unwrap();
    let parsed: PairRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.opake_version, record.opake_version);
    assert_eq!(
        parsed.x25519_ephemeral_key.encoded,
        record.x25519_ephemeral_key.encoded
    );
    assert_eq!(
        parsed.ml_kem_ephemeral_key.encoded,
        record.ml_kem_ephemeral_key.encoded
    );
    assert_eq!(parsed.algo, PAIR_REQUEST_ALGO);
    assert_eq!(parsed.created_at, "2026-03-06T12:00:00Z");
}

#[test]
fn pair_request_uses_atbytes_wire_format() {
    let record = PairRequest::new(&[1u8; 32], &[2u8; 1184], "2026-03-06T00:00:00Z");
    let json = serde_json::to_value(&record).unwrap();
    assert!(json["x25519EphemeralKey"]["$bytes"].is_string());
    assert!(json["mlKemEphemeralKey"]["$bytes"].is_string());
}
