use super::*;

fn key(byte: u8) -> ContentKey {
    ContentKey([byte; 32])
}

const DID_ALICE: &str = "did:plc:alice";
const DID_BOB: &str = "did:plc:bob";

#[test]
fn tag_is_deterministic() {
    let a = derive_workspace_identity_tag(&key(7), DID_ALICE);
    let b = derive_workspace_identity_tag(&key(7), DID_ALICE);
    assert_eq!(a, b);
}

#[test]
fn distinct_keys_derive_distinct_tags() {
    let a = derive_workspace_identity_tag(&key(1), DID_ALICE);
    let b = derive_workspace_identity_tag(&key(2), DID_ALICE);
    assert_ne!(a, b);
}

// spec: workspace-identity § Identity adoption verifies by derivation
#[test]
fn distinct_dids_derive_distinct_tags_under_the_same_key() {
    let a = derive_workspace_identity_tag(&key(1), DID_ALICE);
    let b = derive_workspace_identity_tag(&key(1), DID_BOB);
    assert_ne!(a, b);
}

#[test]
fn tag_is_rkey_charset_valid() {
    let tag = derive_workspace_identity_tag(&key(9), DID_ALICE);
    assert_eq!(tag.len(), 26, "16-byte commitment encodes to 26 chars");
    assert!(
        tag.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
        "tag must stay in the atproto rkey charset: {tag}"
    );
}

// Pinned test vector: a construction drift (label, transcript layout,
// keygen, hash truncation, or encoding) must fail loudly here, because
// the tag is a wire-frozen identity, not an implementation detail.
// spec: workspace-identity § Genesis URI is the workspace identity
#[test]
fn construction_is_pinned() {
    let tag = derive_workspace_identity_tag(&key(0x42), "did:plc:pinned");
    let again = derive_workspace_identity_tag(&key(0x42), "did:plc:pinned");
    assert_eq!(tag, again);
    assert_eq!(tag.len(), 26);
    // KAT pinned from the shipped construction at introduction time. If
    // this fails, the derivation CHANGED — that is a wire break (every
    // existing workspace identity stops verifying), not a test to update
    // casually.
    assert_eq!(tag, PINNED_TAG_FOR_KEY_42_DID_PINNED);
}

const PINNED_TAG_FOR_KEY_42_DID_PINNED: &str = "452upqgt6ql7ci462dvsfcv6bm";

#[test]
fn base32_length_helper_matches_encoding() {
    for len in [0usize, 1, 5, 15, 16, 20] {
        let bytes = vec![0xAB; len];
        assert_eq!(base32_lower(&bytes).len(), base32_len(len));
    }
}
