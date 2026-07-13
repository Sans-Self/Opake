// Sharing is cabinet-only. Grants hand out a single document's content key
// wrapped to one recipient; workspace documents are reached through group
// keys, never grants, so both `share` and `create_pending_share` refuse a
// workspace-bound FileManager before touching the network. These tests pin
// that guard: a workspace-context call errors and writes no record.

use crate::client::{LegacySession, Session, XrpcClient};
use crate::crypto::{generate_content_key, OsRng};
use crate::error::Error;
use crate::manager::types::FileContext;
use crate::opake::Opake;
use crate::storage::{Identity, NoopStorage};
use crate::test_utils::{MockTransport, TestKeys};
use crate::workspace::Workspace;

const CALLER_DID: &str = "did:plc:caller";
const OWNER_DID: &str = "did:plc:wsowner";
const DOC_URI: &str = "at://did:plc:wsowner/app.opake.document/doc1";

fn workspace_context() -> FileContext {
    let group_key = generate_content_key(&mut OsRng);
    FileContext::Workspace(Workspace::from_keyring(
        "at://did:plc:wsowner/app.opake.keyring/genesis".into(),
        "A shared workspace".into(),
        None,
        OWNER_DID.into(),
        group_key,
        1,
        Vec::new(),
        vec![OWNER_DID.into()],
    ))
}

fn opake_with_mock(mock: MockTransport) -> Opake<MockTransport, OsRng, NoopStorage> {
    let session = Session::Legacy(LegacySession {
        did: CALLER_DID.into(),
        handle: "caller.test".into(),
        access_jwt: "test-jwt".into(),
        refresh_jwt: "test-refresh".into(),
    });
    let client = XrpcClient::with_session(mock, "https://pds.test".into(), session);
    let identity = Identity::generate(CALLER_DID, &mut OsRng);
    Opake::new(
        client,
        CALLER_DID.into(),
        identity,
        OsRng,
        NoopStorage,
        || 1_700_000_000_000_000,
    )
    .unwrap()
}

// spec:sharing-grants § Sharing is cabinet-only
#[tokio::test]
async fn share_from_workspace_context_is_refused_and_writes_nothing() {
    let mock = MockTransport::new();
    let mut opake = opake_with_mock(mock.clone());
    let ctx = workspace_context();
    let mut mgr = opake.file_manager(&ctx);

    let recipient = TestKeys::generate("did:plc:recipient");
    let err = mgr
        .share(
            DOC_URI,
            "did:plc:recipient",
            recipient.public_keys(),
            "read",
            None,
        )
        .await
        .unwrap_err();

    assert!(
        matches!(err, Error::InvalidRecord(ref msg) if msg.contains("cabinet")),
        "workspace-context share must be refused as cabinet-only, got: {err:?}",
    );
    // The guard runs before any content-key fetch or grant write: the mock
    // never had a response enqueued, so any network call would surface as a
    // queue-exhausted error rather than the clean refusal above — and no
    // request was captured at all.
    assert!(
        mock.requests().is_empty(),
        "refused share must not touch the PDS, got: {:?}",
        mock.requests(),
    );
}

// spec:sharing-grants § Sharing is cabinet-only
#[tokio::test]
async fn create_pending_share_from_workspace_context_is_refused_and_writes_nothing() {
    let mock = MockTransport::new();
    let mut opake = opake_with_mock(mock.clone());
    let ctx = workspace_context();
    let mut mgr = opake.file_manager(&ctx);

    let err = mgr
        .create_pending_share(DOC_URI, "alice.bsky.social", "read", None)
        .await
        .unwrap_err();

    assert!(
        matches!(err, Error::InvalidRecord(ref msg) if msg.contains("cabinet")),
        "workspace-context pending share must be refused as cabinet-only, got: {err:?}",
    );
    assert!(
        mock.requests().is_empty(),
        "refused pending share must not touch the PDS, got: {:?}",
        mock.requests(),
    );
}
