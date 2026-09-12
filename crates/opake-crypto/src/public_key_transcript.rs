//! Versioned transcripts for account verification and relationship consent.
use sha2::{Digest, Sha256};

use crate::transcript::context_transcript;

/// Decoded hybrid encryption fields shared by signatures and approvals.
pub struct EncryptionKeyFields<'a> {
    pub x25519_public_key: &'a [u8],
    pub x25519_algo: &'a str,
    pub ml_kem_public_key: &'a [u8],
    pub ml_kem_algo: &'a str,
}

/// Closed public-key signature tuple. Version is always the record's version.
pub fn public_key_signature_transcript(
    version: u32,
    did: &str,
    keys: &EncryptionKeyFields<'_>,
    signing_key: &[u8],
    signing_algo: &str,
    created_at: &str,
) -> Vec<u8> {
    context_transcript(
        &format!("at.opake.publicKey/self:v{version}").into_bytes(),
        &[
            did.as_bytes(),
            &version.to_le_bytes(),
            keys.x25519_public_key,
            keys.x25519_algo.as_bytes(),
            keys.ml_kem_public_key,
            keys.ml_kem_algo.as_bytes(),
            signing_key,
            signing_algo.as_bytes(),
            created_at.as_bytes(),
        ],
    )
}

/// Closed approval tuple, scoped to a relationship and its declared version.
pub fn unverified_key_approval_transcript(
    version: u32,
    scope_uri: &str,
    did: &str,
    keys: &EncryptionKeyFields<'_>,
) -> Vec<u8> {
    context_transcript(
        &format!("at.opake.unverified-key-approval:v{version}").into_bytes(),
        &[
            scope_uri.as_bytes(),
            did.as_bytes(),
            keys.x25519_public_key,
            keys.x25519_algo.as_bytes(),
            keys.ml_kem_public_key,
            keys.ml_kem_algo.as_bytes(),
        ],
    )
}

/// Commitment to an already validated encryption bundle; callers supply the
/// containing relationship record's version, not the public-key timestamp.
pub fn unverified_key_approval(
    version: u32,
    scope_uri: &str,
    did: &str,
    keys: &EncryptionKeyFields<'_>,
) -> [u8; 32] {
    Sha256::digest(unverified_key_approval_transcript(
        version, scope_uri, did, keys,
    ))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> EncryptionKeyFields<'static> {
        EncryptionKeyFields {
            x25519_public_key: &[0x00, 0xff],
            x25519_algo: "x25519",
            ml_kem_public_key: &[0x10, 0x11, 0x12],
            ml_kem_algo: "ml-kem-768",
        }
    }

    fn decode_hex(hex: &str) -> Vec<u8> {
        assert_eq!(
            hex.len() % 2,
            0,
            "hex test vectors must have complete bytes"
        );
        hex.as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let digit = |byte: u8| match byte {
                    b'0'..=b'9' => byte - b'0',
                    b'a'..=b'f' => byte - b'a' + 10,
                    b'A'..=b'F' => byte - b'A' + 10,
                    _ => panic!("hex test vector contains a non-hex digit"),
                };
                (digit(pair[0]) << 4) | digit(pair[1])
            })
            .collect()
    }

    // Wire-frozen cross-language vector. The non-palindromic version makes
    // the required u32 little-endian encoding observable in the bytes.
    // spec: account-verification § The signature covers a fixed, versioned transcript that names the account
    #[test]
    fn public_key_signature_transcript_is_pinned() {
        let transcript = public_key_signature_transcript(
            0x0102_0304,
            "did:plc:alice",
            &fields(),
            &[0xaa, 0xbb],
            "ed25519",
            "2026-09-12T00:00:00Z",
        );

        assert_eq!(
            transcript,
            decode_hex(concat!(
                "61742e6f70616b652e7075626c69634b65792f73656c663a7631363930393036300900",
                "00000d0000006469643a706c633a616c69636504000000040302010200000000ff0600",
                "0000783235353139030000001011120a0000006d6c2d6b656d2d37363802000000aabb",
                "070000006564323535313914000000323032362d30392d31325430303a30303a30305a"
            ))
        );
    }

    // The commitment vector pins both the transcript and its SHA-256 consumer.
    // spec: account-verification § Key-bound approval is carried by the relationship's records
    #[test]
    fn unverified_key_approval_is_pinned() {
        let version = 0x0102_0304;
        let scope = "at://did:plc:alice/at.opake.keyring/workspace";
        let did = "did:plc:alice";
        let transcript = unverified_key_approval_transcript(version, scope, did, &fields());

        assert_eq!(
            transcript,
            decode_hex(concat!(
                "61742e6f70616b652e756e76657269666965642d6b65792d617070726f76616c",
                "3a763136393039303630060000002d00000061743a2f2f6469643a706c633a61",
                "6c6963652f61742e6f70616b652e6b657972696e672f776f726b73706163650d",
                "0000006469643a706c633a616c6963650200000000ff06000000783235353139",
                "030000001011120a0000006d6c2d6b656d2d373638"
            ))
        );
        assert_eq!(
            unverified_key_approval(version, scope, did, &fields()),
            [
                0xdc, 0xa6, 0xa6, 0x28, 0x60, 0x63, 0xa0, 0x63, 0x4f, 0x11, 0xed, 0x09, 0xc0, 0xa4,
                0xf3, 0xdf, 0x83, 0x59, 0x88, 0xa8, 0x50, 0xc3, 0x5c, 0x22, 0x23, 0x1d, 0x79, 0x3b,
                0xb3, 0xb7, 0xf5, 0x8f,
            ]
        );
    }
}
