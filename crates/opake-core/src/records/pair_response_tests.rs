use super::*;
use crate::atproto::AtBytes;
use crate::records::SCHEMA_VERSION;

#[test]
fn pair_response_roundtrips_through_json() {
    let record = PairResponse {
        opake_version: SCHEMA_VERSION,
        request: "at://did:plc:test/at.opake.pairRequest/abc123".into(),
        wrapped_key: WrappedKey {
            did: "did:plc:test".into(),
            ciphertext: AtBytes {
                encoded: "AAAA".into(),
            },
            algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
        },
        ciphertext: AtBytes {
            encoded: "BBBB".into(),
        },
        nonce: AtBytes {
            encoded: "CCCC".into(),
        },
        algo: "aes-256-gcm".into(),
        created_at: "2026-03-06T12:00:00Z".into(),
    };

    let json = serde_json::to_string(&record).unwrap();
    let parsed: PairResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.opake_version, SCHEMA_VERSION);
    assert_eq!(parsed.request, record.request);
    assert_eq!(parsed.wrapped_key.did, "did:plc:test");
    assert_eq!(parsed.wrapped_key.algo, "x25519-mlkem768-hkdf-a256kw-v2");
    assert_eq!(parsed.algo, "aes-256-gcm");
    assert_eq!(parsed.created_at, "2026-03-06T12:00:00Z");
}

#[test]
fn pair_response_uses_atbytes_wire_format() {
    let record = PairResponse {
        opake_version: SCHEMA_VERSION,
        request: "at://did:plc:test/at.opake.pairRequest/abc123".into(),
        wrapped_key: WrappedKey {
            did: "did:plc:test".into(),
            ciphertext: AtBytes {
                encoded: "AAAA".into(),
            },
            algo: "x25519-mlkem768-hkdf-a256kw-v2".into(),
        },
        ciphertext: AtBytes {
            encoded: "BBBB".into(),
        },
        nonce: AtBytes {
            encoded: "CCCC".into(),
        },
        algo: "aes-256-gcm".into(),
        created_at: "2026-03-06T00:00:00Z".into(),
    };

    let json = serde_json::to_value(&record).unwrap();
    assert!(json["ciphertext"]["$bytes"].is_string());
    assert!(json["nonce"]["$bytes"].is_string());
    assert!(json["wrappedKey"]["ciphertext"]["$bytes"].is_string());
}
