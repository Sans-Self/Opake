use super::*;
use crate::crypto::generate_content_key;
use crate::crypto::OsRng;

#[test]
fn from_keyring_preserves_fields() {
    let gk = generate_content_key(&mut OsRng);
    let ws = Workspace::from_keyring(
        "at://did:plc:owner/app.opake.keyring/abc123".into(),
        "My Project".into(),
        Some("A shared workspace".into()),
        "did:plc:owner".into(),
        gk,
        3,
        Vec::new(),
    );

    assert_eq!(
        ws.keyring_uri(),
        "at://did:plc:owner/app.opake.keyring/abc123"
    );
    assert_eq!(ws.name, "My Project");
    assert_eq!(ws.description.as_deref(), Some("A shared workspace"));
    assert_eq!(ws.owner_did, "did:plc:owner");
    assert_eq!(ws.rotation, 3);
}

#[test]
fn root_rkey_derives_from_keyring_uri() {
    let gk = generate_content_key(&mut OsRng);
    let ws = Workspace::from_keyring(
        "at://did:plc:owner/app.opake.keyring/abc123".into(),
        "Test".into(),
        None,
        "did:plc:owner".into(),
        gk,
        1,
        Vec::new(),
    );

    assert_eq!(ws.root_rkey(), "ws-abc123");
}

#[test]
fn root_directory_uri_is_deterministic() {
    let gk = generate_content_key(&mut OsRng);
    let ws = Workspace::from_keyring(
        "at://did:plc:owner/app.opake.keyring/abc123".into(),
        "Test".into(),
        None,
        "did:plc:owner".into(),
        gk,
        1,
        Vec::new(),
    );

    assert_eq!(
        ws.root_directory_uri(),
        "at://did:plc:owner/app.opake.directory/ws-abc123"
    );
}
