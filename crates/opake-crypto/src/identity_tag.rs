//! Workspace identity tag derivation.
//!
//! The genesis keyring's rkey is not an address that names a workspace —
//! it is a value the workspace's genesis group key can *prove*. The key
//! and the owner DID seed an Ed25519 identity keypair, and the rkey is a
//! base32 encoding of the public key's hash. Holding a member wrap is
//! therefore sufficient to verify, offline, that a declared lineage
//! anchor really belongs to the key material it arrived with; an
//! identity for a key you do not hold is a preimage away.
//!
//! The keypair's private half is used by no current operation: it is
//! derived, its public key is taken, and it is dropped (zeroized via
//! ed25519-dalek's `zeroize` feature) before this function returns. The
//! public-key commitment — rather than a bare KDF output — is what keeps
//! workspace signatures available later as an additive change.
// spec: workspace-identity § Genesis URI is the workspace identity

use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::transcript::{context_transcript, WORKSPACE_IDENTITY_LABEL};
use crate::ContentKey;

/// Length of the tag commitment in bytes (128 bits).
const IDENTITY_TAG_LEN: usize = 16;

/// RFC 4648 base32 alphabet, lowercase, unpadded — rkey-charset-safe.
const BASE32_LOWER: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Derive the workspace identity tag — the genesis keyring rkey — from
/// the genesis (rotation-0) group key and the owner's DID.
///
/// Folding the DID into the derivation ties both segments of the genesis
/// URI to key possession: a forged rkey fails on the key, a forged owner
/// attribution fails on the DID, and neither check consults any host.
// spec: workspace-identity § Identity adoption verifies by derivation
pub fn derive_workspace_identity_tag(genesis_group_key: &ContentKey, owner_did: &str) -> String {
    let info = context_transcript(WORKSPACE_IDENTITY_LABEL, &[owner_did.as_bytes()]);

    let hk = Hkdf::<Sha256>::new(None, &genesis_group_key.0);
    let mut seed = Zeroizing::new([0u8; 32]);
    hk.expand(&info, seed.as_mut())
        .expect("32 bytes is a valid HKDF-SHA256 output length");

    // SigningKey zeroizes on drop (ed25519-dalek `zeroize` feature); the
    // private half is used nowhere — only the public key leaves this scope.
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
    let public_key = signing_key.verifying_key();
    drop(signing_key);

    let digest = Sha256::digest(public_key.as_bytes());
    base32_lower(&digest[..IDENTITY_TAG_LEN])
}

const fn base32_len(bytes: usize) -> usize {
    (bytes * 8).div_ceil(5)
}

fn base32_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(base32_len(bytes.len()));
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(BASE32_LOWER[((buffer >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(BASE32_LOWER[((buffer << (5 - bits)) & 0x1f) as usize] as char);
    }
    out
}

#[cfg(test)]
#[path = "identity_tag_tests.rs"]
mod tests;
