//! AAD context for content and metadata sealing.
//!
//! Every AES-256-GCM ciphertext is bound to the lineage anchor of the
//! record it belongs to and the type of field it seals. The anchor is
//! chain-constant, so a ciphertext copied verbatim into a superseding
//! record still authenticates; the type is slot-derived, so a ciphertext
//! presented in the wrong field fails even under the right key.
// spec: document-crypto § Ciphertexts are AAD-bound to their lineage anchor and type

use crate::transcript::{context_transcript, SEAL_AAD_LABEL};
use crate::PAIR_RESPONSE_SENTINEL;

/// The field a ciphertext seals. One content key covers a document's blob
/// and its metadata, so the type tag — not key uniqueness — is what makes
/// a blob↔metadata swap inside one record fail authentication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SealType {
    DocumentBlob,
    DocumentMetadata,
    KeyringMetadata,
    DirectoryMetadata,
    GrantMetadata,
    PairIdentity,
}

impl SealType {
    pub(crate) fn tag(self) -> &'static str {
        match self {
            SealType::DocumentBlob => "document-blob",
            SealType::DocumentMetadata => "document-metadata",
            SealType::KeyringMetadata => "keyring-metadata",
            SealType::DirectoryMetadata => "directory-metadata",
            SealType::GrantMetadata => "grant-metadata",
            SealType::PairIdentity => "pair-identity",
        }
    }
}

/// AAD context: which object a ciphertext belongs to, and which field it
/// seals. `anchor` is the record's lineage anchor — the chain's genesis
/// URI, which is the record's own URI when the record is genesis or never
/// chains
/// (`spec:lineage § Lineage is the chain's genesis URI, carried on every supersede`).
#[derive(Debug, Clone, Copy)]
pub struct SealContext<'a> {
    anchor: &'a str,
    seal_type: SealType,
}

impl<'a> SealContext<'a> {
    pub fn new(anchor: &'a str, seal_type: SealType) -> Self {
        Self { anchor, seal_type }
    }

    /// Pairing identity blob — sentinel anchor, no record URI involved.
    pub fn pair_identity() -> SealContext<'static> {
        SealContext {
            anchor: PAIR_RESPONSE_SENTINEL,
            seal_type: SealType::PairIdentity,
        }
    }

    pub(crate) fn aad(&self) -> Vec<u8> {
        context_transcript(
            SEAL_AAD_LABEL,
            &[self.anchor.as_bytes(), self.seal_type.tag().as_bytes()],
        )
    }
}

#[cfg(test)]
#[path = "seal_context_tests.rs"]
mod tests;
