use super::*;

#[test]
fn pair_request_new_sets_defaults() {
    let record = PairRequest::new(&[42u8; 32], "2026-03-06T00:00:00Z");
    assert_eq!(record.opake_version, SCHEMA_VERSION);
    assert_eq!(record.algo, "x25519");
    assert_eq!(record.created_at, "2026-03-06T00:00:00Z");
}

#[test]
fn pair_request_roundtrips_through_json() {
    let record = PairRequest::new(&[7u8; 32], "2026-03-06T12:00:00Z");
    let json = serde_json::to_string(&record).unwrap();
    let parsed: PairRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.opake_version, record.opake_version);
    assert_eq!(parsed.ephemeral_key.encoded, record.ephemeral_key.encoded);
    assert_eq!(parsed.algo, "x25519");
    assert_eq!(parsed.created_at, "2026-03-06T12:00:00Z");
}

#[test]
fn pair_request_uses_atbytes_wire_format() {
    let record = PairRequest::new(&[1u8; 32], "2026-03-06T00:00:00Z");
    let json = serde_json::to_value(&record).unwrap();
    assert!(json["ephemeralKey"]["$bytes"].is_string());
}
