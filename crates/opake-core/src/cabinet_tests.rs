use super::*;
use crate::crypto::OsRng;
use crate::storage::Identity;

#[test]
fn from_identity_decodes_keys() {
    let identity = Identity::generate("did:plc:test", &mut OsRng);
    let cabinet = Cabinet::from_identity(&identity).unwrap();

    assert_eq!(cabinet.did, "did:plc:test");
    assert_eq!(cabinet.public_key, identity.public_key_bytes().unwrap());
    assert_eq!(cabinet.private_key, identity.private_key_bytes().unwrap());
}

#[test]
fn root_directory_uri_uses_self_rkey() {
    let identity = Identity::generate("did:plc:test", &mut OsRng);
    let cabinet = Cabinet::from_identity(&identity).unwrap();

    assert_eq!(
        cabinet.root_directory_uri(),
        "at://did:plc:test/app.opake.directory/self"
    );
}
