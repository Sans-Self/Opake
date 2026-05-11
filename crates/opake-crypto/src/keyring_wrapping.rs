use aes_kw::KekAes256;

use crate::error::Error;
use crate::{ContentKey, CONTENT_KEY_LEN, WRAPPED_KEY_LEN};

/// Wrap a per-document content key under a keyring's group key (symmetric AES-KW).
///
/// Returns 40 bytes: the 32-byte content key + 8-byte AES-KW integrity tag.
pub fn wrap_content_key_for_keyring(
    content_key: &ContentKey,
    group_key: &ContentKey,
) -> Result<Vec<u8>, Error> {
    let kek = KekAes256::new((&group_key.0).into());
    kek.wrap_vec(&content_key.0)
        .map_err(|_| Error::KeyWrap("AES key wrap under group key failed".into()))
}

/// Unwrap a per-document content key using the keyring's group key.
pub fn unwrap_content_key_from_keyring(
    wrapped: &[u8],
    group_key: &ContentKey,
) -> Result<ContentKey, Error> {
    if wrapped.len() != WRAPPED_KEY_LEN {
        return Err(Error::Decryption(format!(
            "keyring-wrapped key is {} bytes, expected {WRAPPED_KEY_LEN}",
            wrapped.len()
        )));
    }

    let kek = KekAes256::new((&group_key.0).into());
    let unwrapped = kek
        .unwrap_vec(wrapped)
        .map_err(|_| Error::Decryption("AES key unwrap under group key failed".into()))?;

    let mut key = [0u8; CONTENT_KEY_LEN];
    key.copy_from_slice(&unwrapped);
    Ok(ContentKey(key))
}
