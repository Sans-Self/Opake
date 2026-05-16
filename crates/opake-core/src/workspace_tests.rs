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

// `root_rkey` and `root_directory_uri` were removed in the lean federation
// pivot. Workspace roots are now TID-rkeyed and discovered via the indexer's
// `chain_heads` table; there's no deterministic derivation to assert against.
