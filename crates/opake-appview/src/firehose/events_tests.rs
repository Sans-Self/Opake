use super::*;

fn grant_event_json(operation: &str) -> String {
    format!(
        r#"{{
  "did": "did:plc:owner123",
  "time_us": 1709330400000000,
  "kind": "commit",
  "commit": {{
    "rev": "3l3qo2vutsw2b",
    "operation": "{operation}",
    "collection": "app.opake.cloud.grant",
    "rkey": "3abc",
    "record": {{
      "version": 1,
      "document": "at://did:plc:owner123/app.opake.cloud.document/3xyz",
      "recipient": "did:plc:recipient456",
      "wrappedKey": {{
        "did": "did:plc:recipient456",
        "ciphertext": {{ "$bytes": "AAAA" }},
        "algo": "x25519-hkdf-a256kw"
      }},
      "createdAt": "2026-03-01T12:00:00Z"
    }},
    "cid": "bafyabc"
  }}
}}"#
    )
}

fn keyring_event_json(operation: &str) -> String {
    format!(
        r#"{{
  "did": "did:plc:owner123",
  "time_us": 1709330500000000,
  "kind": "commit",
  "commit": {{
    "rev": "3l3qo2vutsw2b",
    "operation": "{operation}",
    "collection": "app.opake.cloud.keyring",
    "rkey": "3def",
    "record": {{
      "version": 1,
      "name": "family-photos",
      "algo": "aes-256-gcm",
      "members": [
        {{
          "did": "did:plc:alice",
          "ciphertext": {{ "$bytes": "AAAA" }},
          "algo": "x25519-hkdf-a256kw"
        }},
        {{
          "did": "did:plc:bob",
          "ciphertext": {{ "$bytes": "BBBB" }},
          "algo": "x25519-hkdf-a256kw"
        }}
      ],
      "rotation": 0,
      "createdAt": "2026-03-01T12:00:00Z"
    }},
    "cid": "bafydef"
  }}
}}"#
    )
}

fn delete_event_json(collection: &str, rkey: &str) -> String {
    format!(
        r#"{{
  "did": "did:plc:owner123",
  "time_us": 1709330600000000,
  "kind": "commit",
  "commit": {{
    "rev": "3l3qo2vutsw2b",
    "operation": "delete",
    "collection": "{collection}",
    "rkey": "{rkey}"
  }}
}}"#
    )
}

#[test]
fn parses_grant_create() {
    let json = grant_event_json("create");
    let (event, time_us) = parse_event(&json).unwrap();
    assert_eq!(time_us, 1709330400000000);

    match event {
        IndexableEvent::UpsertGrant {
            uri,
            owner_did,
            recipient_did,
            document_uri,
            ..
        } => {
            assert_eq!(uri, "at://did:plc:owner123/app.opake.cloud.grant/3abc");
            assert_eq!(owner_did, "did:plc:owner123");
            assert_eq!(recipient_did, "did:plc:recipient456");
            assert_eq!(
                document_uri,
                "at://did:plc:owner123/app.opake.cloud.document/3xyz"
            );
        }
        other => panic!("expected UpsertGrant, got {other:?}"),
    }
}

#[test]
fn parses_grant_update() {
    let json = grant_event_json("update");
    let (event, _) = parse_event(&json).unwrap();
    assert!(matches!(event, IndexableEvent::UpsertGrant { .. }));
}

#[test]
fn parses_grant_delete() {
    let json = delete_event_json("app.opake.cloud.grant", "3abc");
    let (event, _) = parse_event(&json).unwrap();
    match event {
        IndexableEvent::DeleteGrant { uri } => {
            assert_eq!(uri, "at://did:plc:owner123/app.opake.cloud.grant/3abc");
        }
        other => panic!("expected DeleteGrant, got {other:?}"),
    }
}

#[test]
fn parses_keyring_create() {
    let json = keyring_event_json("create");
    let (event, time_us) = parse_event(&json).unwrap();
    assert_eq!(time_us, 1709330500000000);

    match event {
        IndexableEvent::UpsertKeyring {
            uri,
            owner_did,
            name,
            member_dids,
        } => {
            assert_eq!(uri, "at://did:plc:owner123/app.opake.cloud.keyring/3def");
            assert_eq!(owner_did, "did:plc:owner123");
            assert_eq!(name, "family-photos");
            assert_eq!(member_dids, vec!["did:plc:alice", "did:plc:bob"]);
        }
        other => panic!("expected UpsertKeyring, got {other:?}"),
    }
}

#[test]
fn parses_keyring_delete() {
    let json = delete_event_json("app.opake.cloud.keyring", "3def");
    let (event, _) = parse_event(&json).unwrap();
    assert!(matches!(event, IndexableEvent::DeleteKeyring { .. }));
}

#[test]
fn ignores_identity_events() {
    let json = r#"{"did":"did:plc:abc","time_us":123,"kind":"identity"}"#;
    assert!(parse_event(json).is_none());
}

#[test]
fn ignores_unknown_collections() {
    let json = r#"{
  "did": "did:plc:abc",
  "time_us": 123,
  "kind": "commit",
  "commit": {
    "rev": "abc",
    "operation": "create",
    "collection": "app.bsky.feed.post",
    "rkey": "3abc",
    "record": {"text": "hello"},
    "cid": "bafyabc"
  }
}"#;
    assert!(parse_event(json).is_none());
}

#[test]
fn ignores_malformed_json() {
    assert!(parse_event("not json at all").is_none());
}

#[test]
fn ignores_grant_with_invalid_record() {
    // record is present but doesn't match Grant schema
    let json = r#"{
  "did": "did:plc:owner",
  "time_us": 123,
  "kind": "commit",
  "commit": {
    "rev": "abc",
    "operation": "create",
    "collection": "app.opake.cloud.grant",
    "rkey": "3abc",
    "record": {"garbage": true},
    "cid": "bafyabc"
  }
}"#;
    assert!(parse_event(json).is_none());
}
