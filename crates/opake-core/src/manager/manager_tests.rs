use crate::manager::types::{FileContext, MutationOutcome};

#[test]
fn file_context_owner_did_cabinet() {
    use crate::cabinet::Cabinet;
    use crate::crypto::OsRng;
    use crate::storage::Identity;

    let id = Identity::generate("did:plc:alice", &mut OsRng);
    let cabinet = Cabinet::from_identity(&id).unwrap();
    let ctx = FileContext::Cabinet(cabinet);

    assert_eq!(ctx.owner_did(), "did:plc:alice");
    assert!(ctx.is_cabinet());
    assert!(!ctx.is_workspace());
}

#[test]
fn file_context_owner_did_workspace() {
    use crate::crypto::{generate_content_key, OsRng};
    use crate::workspace::Workspace;

    let gk = generate_content_key(&mut OsRng);
    let ws = Workspace::from_keyring(
        "at://did:plc:bob/app.opake.keyring/xyz".into(),
        "Bob's WS".into(),
        None,
        "did:plc:bob".into(),
        gk,
        1,
    );
    let ctx = FileContext::Workspace(ws);

    assert_eq!(ctx.owner_did(), "did:plc:bob");
    assert!(ctx.is_workspace());
    assert!(!ctx.is_cabinet());
}

#[test]
fn mutation_outcome_predicates() {
    let applied = MutationOutcome::Applied;
    assert!(applied.is_applied());
    assert!(!applied.is_proposed());

    let proposed = MutationOutcome::Proposed {
        update_uri: "at://did:plc:x/app.opake.directoryUpdate/tid".into(),
    };
    assert!(!proposed.is_applied());
    assert!(proposed.is_proposed());
}
