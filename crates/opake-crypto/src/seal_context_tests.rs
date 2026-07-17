use super::{SealContext, SealType};

#[test]
fn aad_differs_by_type_under_one_anchor() {
    let uri = "at://did:plc:x/at.opake.document/3abc";
    let blob = SealContext::new(uri, SealType::DocumentBlob).aad();
    let metadata = SealContext::new(uri, SealType::DocumentMetadata).aad();
    assert_ne!(blob, metadata);
}

#[test]
fn aad_differs_by_anchor_under_one_type() {
    let a = SealContext::new("at://did:plc:x/at.opake.document/a", SealType::DocumentBlob).aad();
    let b = SealContext::new("at://did:plc:x/at.opake.document/b", SealType::DocumentBlob).aad();
    assert_ne!(a, b);
}

#[test]
fn aad_is_deterministic() {
    let make = || SealContext::new("at://did:plc:x/at.opake.keyring/g", SealType::KeyringMetadata);
    assert_eq!(make().aad(), make().aad());
}

#[test]
fn pair_identity_uses_the_sentinel() {
    let sentinel = SealContext::pair_identity().aad();
    let explicit = SealContext::new(crate::PAIR_RESPONSE_SENTINEL, SealType::PairIdentity).aad();
    assert_eq!(sentinel, explicit);
}
