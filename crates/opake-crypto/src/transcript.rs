//! Injective context-transcript encoding.
//!
//! Both the HKDF `info` for key wraps and the AAD for content/metadata
//! AEAD are byte strings committing to a tuple of context fields. Joining
//! fields with a delimiter is not injective when a field may contain the
//! delimiter (`did:web` identifiers legally contain hyphens), and both
//! transcripts feed cryptographic operations where an encoding collision
//! is a cross-context collision. This encoder is the single producer of
//! both byte strings.
//!
//! Layout: `label ‖ u32-LE field count ‖ (u32-LE length ‖ bytes)*`.
//! Length-prefixing makes the encoding injective by construction — no
//! arrangement of field contents can imitate another arrangement, and the
//! labels domain-separate the two consumers from each other.
// spec: document-crypto § Wraps are AEAD-bound to their record context

/// Label for HKDF `info` transcripts (key wrapping).
pub(crate) const WRAP_INFO_LABEL: &[u8] = b"opake-wrap-info";

/// Label for AEAD associated-data transcripts (content/metadata sealing).
pub(crate) const SEAL_AAD_LABEL: &[u8] = b"opake-seal-aad";

/// Encode `fields` under `label` as an injective byte transcript.
pub(crate) fn context_transcript(label: &[u8], fields: &[&[u8]]) -> Vec<u8> {
    let payload_len: usize = fields.iter().map(|f| 4 + f.len()).sum();
    let mut out = Vec::with_capacity(label.len() + 4 + payload_len);
    out.extend_from_slice(label);
    out.extend_from_slice(&u32::try_from(fields.len()).expect("field count fits u32").to_le_bytes());
    for field in fields {
        let len = u32::try_from(field.len()).expect("field length fits u32");
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(field);
    }
    out
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
