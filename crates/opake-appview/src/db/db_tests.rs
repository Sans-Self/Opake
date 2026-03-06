use super::*;
use grants::IndexedGrant;

fn test_db() -> Database {
    Database::open_in_memory().unwrap()
}

fn make_grant(uri: &str, recipient: &str, owner: &str, doc_uri: &str) -> IndexedGrant {
    IndexedGrant {
        uri: uri.into(),
        owner_did: owner.into(),
        recipient_did: recipient.into(),
        document_uri: doc_uri.into(),
        permissions: Some("read".into()),
        note: None,
        created_at: "2026-03-01T12:00:00Z".into(),
        indexed_at: "2026-03-01T12:00:01Z".into(),
    }
}

#[test]
fn grant_upsert_and_query() {
    let db = test_db();
    let grant = make_grant(
        "at://did:plc:owner/app.opake.grant/3abc",
        "did:plc:recipient",
        "did:plc:owner",
        "at://did:plc:owner/app.opake.document/3xyz",
    );

    db.with_conn(|c| grants::upsert_grant(c, &grant)).unwrap();

    let inbox = db
        .with_conn(|c| grants::list_inbox(c, "did:plc:recipient", 50, None))
        .unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].uri, grant.uri);
    assert_eq!(inbox[0].document_uri, grant.document_uri);
}

#[test]
fn grant_upsert_overwrites() {
    let db = test_db();
    let mut grant = make_grant(
        "at://did:plc:owner/app.opake.grant/3abc",
        "did:plc:recipient",
        "did:plc:owner",
        "at://did:plc:owner/app.opake.document/3xyz",
    );
    db.with_conn(|c| grants::upsert_grant(c, &grant)).unwrap();

    grant.note = Some("updated note".into());
    db.with_conn(|c| grants::upsert_grant(c, &grant)).unwrap();

    let inbox = db
        .with_conn(|c| grants::list_inbox(c, "did:plc:recipient", 50, None))
        .unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].note.as_deref(), Some("updated note"));
}

#[test]
fn grant_delete() {
    let db = test_db();
    let grant = make_grant(
        "at://did:plc:owner/app.opake.grant/3abc",
        "did:plc:recipient",
        "did:plc:owner",
        "at://did:plc:owner/app.opake.document/3xyz",
    );
    db.with_conn(|c| grants::upsert_grant(c, &grant)).unwrap();
    db.with_conn(|c| grants::delete_grant(c, &grant.uri))
        .unwrap();

    let inbox = db
        .with_conn(|c| grants::list_inbox(c, "did:plc:recipient", 50, None))
        .unwrap();
    assert!(inbox.is_empty());
}

#[test]
fn grant_pagination() {
    let db = test_db();

    for i in 0..5 {
        let grant = IndexedGrant {
            uri: format!("at://did:plc:owner/app.opake.grant/{i}"),
            owner_did: "did:plc:owner".into(),
            recipient_did: "did:plc:me".into(),
            document_uri: format!("at://did:plc:owner/app.opake.document/{i}"),
            permissions: None,
            note: None,
            created_at: "2026-03-01T12:00:00Z".into(),
            indexed_at: format!("2026-03-01T12:00:0{i}Z"),
        };
        db.with_conn(|c| grants::upsert_grant(c, &grant)).unwrap();
    }

    // First page: 2 items
    let page1 = db
        .with_conn(|c| grants::list_inbox(c, "did:plc:me", 2, None))
        .unwrap();
    assert_eq!(page1.len(), 2);
    // Newest first
    assert!(page1[0].indexed_at > page1[1].indexed_at);

    // Second page using cursor from last item of page 1
    let cursor = grants::encode_cursor(&page1[1]);
    let page2 = db
        .with_conn(|c| grants::list_inbox(c, "did:plc:me", 2, Some(&cursor)))
        .unwrap();
    assert_eq!(page2.len(), 2);
    assert!(page2[0].indexed_at < page1[1].indexed_at);
}

#[test]
fn keyring_upsert_and_query() {
    let db = test_db();
    let members = vec!["did:plc:alice".to_string(), "did:plc:bob".to_string()];

    db.with_conn(|c| {
        keyrings::upsert_keyring_members(
            c,
            "at://did:plc:owner/app.opake.keyring/3def",
            "did:plc:owner",
            "family-photos",
            &members,
            "2026-03-01T12:00:00Z",
        )
    })
    .unwrap();

    let alice_keyrings = db
        .with_conn(|c| keyrings::list_keyrings_for_member(c, "did:plc:alice", 50, None))
        .unwrap();
    assert_eq!(alice_keyrings.len(), 1);
    assert_eq!(alice_keyrings[0].keyring_name, "family-photos");

    let bob_keyrings = db
        .with_conn(|c| keyrings::list_keyrings_for_member(c, "did:plc:bob", 50, None))
        .unwrap();
    assert_eq!(bob_keyrings.len(), 1);

    // Charlie is not a member
    let charlie_keyrings = db
        .with_conn(|c| keyrings::list_keyrings_for_member(c, "did:plc:charlie", 50, None))
        .unwrap();
    assert!(charlie_keyrings.is_empty());
}

#[test]
fn keyring_update_replaces_members() {
    let db = test_db();
    let uri = "at://did:plc:owner/app.opake.keyring/3def";

    // Initially: alice + bob
    db.with_conn(|c| {
        keyrings::upsert_keyring_members(
            c,
            uri,
            "did:plc:owner",
            "family-photos",
            &["did:plc:alice".into(), "did:plc:bob".into()],
            "2026-03-01T12:00:00Z",
        )
    })
    .unwrap();

    // Update: bob removed, charlie added
    db.with_conn(|c| {
        keyrings::upsert_keyring_members(
            c,
            uri,
            "did:plc:owner",
            "family-photos",
            &["did:plc:alice".into(), "did:plc:charlie".into()],
            "2026-03-01T13:00:00Z",
        )
    })
    .unwrap();

    // Bob should no longer see it
    let bob = db
        .with_conn(|c| keyrings::list_keyrings_for_member(c, "did:plc:bob", 50, None))
        .unwrap();
    assert!(bob.is_empty());

    // Charlie should see it
    let charlie = db
        .with_conn(|c| keyrings::list_keyrings_for_member(c, "did:plc:charlie", 50, None))
        .unwrap();
    assert_eq!(charlie.len(), 1);
}

#[test]
fn keyring_delete() {
    let db = test_db();
    let uri = "at://did:plc:owner/app.opake.keyring/3def";

    db.with_conn(|c| {
        keyrings::upsert_keyring_members(
            c,
            uri,
            "did:plc:owner",
            "family-photos",
            &["did:plc:alice".into()],
            "2026-03-01T12:00:00Z",
        )
    })
    .unwrap();

    db.with_conn(|c| keyrings::delete_keyring(c, uri)).unwrap();

    let alice = db
        .with_conn(|c| keyrings::list_keyrings_for_member(c, "did:plc:alice", 50, None))
        .unwrap();
    assert!(alice.is_empty());
}

#[test]
fn cursor_roundtrip() {
    let db = test_db();

    // No cursor initially
    let initial = db.with_conn(|c| cursor::load_cursor(c)).unwrap();
    assert!(initial.is_none());

    // Save and load
    db.with_conn(|c| cursor::save_cursor(c, 1709330400000000))
        .unwrap();
    let loaded = db.with_conn(|c| cursor::load_cursor(c)).unwrap();
    assert_eq!(loaded, Some(1709330400000000));

    // Update
    db.with_conn(|c| cursor::save_cursor(c, 1709330500000000))
        .unwrap();
    let updated = db.with_conn(|c| cursor::load_cursor(c)).unwrap();
    assert_eq!(updated, Some(1709330500000000));
}
