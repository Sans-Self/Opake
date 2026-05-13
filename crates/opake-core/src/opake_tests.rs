use super::*;
use crate::crypto::{generate_content_key, OsRng};
use crate::storage::{Identity, NoopStorage};
use crate::test_utils::MockTransport;

fn test_now_micros() -> u64 {
    1_700_000_000_000_000
}

fn make_test_opake() -> Opake<MockTransport, OsRng, NoopStorage> {
    let transport = MockTransport::new();
    let client = crate::client::XrpcClient::new(transport, "https://pds.example.com".into());
    let identity = Identity::generate("did:plc:test", &mut OsRng);
    Opake::new(
        client,
        "did:plc:test".into(),
        identity,
        OsRng,
        NoopStorage,
        test_now_micros,
    )
    .unwrap()
}

#[test]
fn did_returns_identity_did() {
    let opake = make_test_opake();
    assert_eq!(opake.did(), "did:plc:test");
}

#[test]
fn now_derives_rfc3339_from_injected_micros() {
    let opake = make_test_opake();
    // `test_now_micros()` returns 1_700_000_000_000_000 µs.
    assert_eq!(opake.now(), "2023-11-14T22:13:20.000000Z");
}

#[test]
fn cabinet_context_produces_cabinet() {
    let opake = make_test_opake();
    let context = opake.cabinet_context().unwrap();
    assert!(context.is_cabinet());
}

#[test]
fn cabinet_file_manager_is_owner() {
    let mut opake = make_test_opake();
    let context = opake.cabinet_context().unwrap();
    let mgr = opake.file_manager(&context);
    assert!(mgr.is_owner());
}

#[test]
fn workspace_file_manager_non_owner() {
    let gk = generate_content_key(&mut OsRng);
    let ws = Workspace::from_keyring(
        "at://did:plc:owner/app.opake.keyring/abc".into(),
        "Test WS".into(),
        None,
        "did:plc:owner".into(),
        gk,
        1,
        Vec::new(),
    );

    let mut opake = make_test_opake();
    let context = FileContext::Workspace(ws);
    let mgr = opake.file_manager(&context);

    assert!(mgr.context().is_workspace());
    // Caller is "did:plc:test", owner is "did:plc:owner" → not owner.
    assert!(!mgr.is_owner());
}

#[test]
fn workspace_owner_is_detected() {
    let gk = generate_content_key(&mut OsRng);
    let ws = Workspace::from_keyring(
        "at://did:plc:test/app.opake.keyring/abc".into(),
        "My WS".into(),
        None,
        "did:plc:test".into(), // Same as the identity DID
        gk,
        1,
        Vec::new(),
    );

    let mut opake = make_test_opake();
    let context = FileContext::Workspace(ws);
    let mgr = opake.file_manager(&context);

    assert!(mgr.is_owner());
}

#[test]
fn workspace_admin_preserves_workspace() {
    let gk = generate_content_key(&mut OsRng);
    let ws = Workspace::from_keyring(
        "at://did:plc:owner/app.opake.keyring/abc".into(),
        "Admin WS".into(),
        None,
        "did:plc:owner".into(),
        gk,
        1,
        Vec::new(),
    );

    let mut opake = make_test_opake();
    let admin = opake.workspace_admin(&ws);

    assert_eq!(admin.workspace().name, "Admin WS");
    assert_eq!(
        admin.workspace().keyring_uri(),
        "at://did:plc:owner/app.opake.keyring/abc"
    );
}


