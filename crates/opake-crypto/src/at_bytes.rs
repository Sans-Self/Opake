// `AtBytes`: atproto's `{ "$bytes": "<base64>" }` JSON convention.
//
// Every primitive that emits bytes for the wire (wrapped keys, encrypted
// metadata, ciphertexts) renders through this wrapper.

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// Binary data in atproto JSON: `{ "$bytes": "<base64>" }`.
///
/// The PDS stores bytes as CBOR internally and re-encodes to *unpadded*
/// base64 on JSON read, even if we uploaded *padded* base64. Use
/// [`AtBytes::decode`] to handle both forms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtBytes {
    #[serde(rename = "$bytes")]
    pub encoded: String,
}

impl AtBytes {
    /// Construct from raw bytes, base64-encoding them for the wire format.
    pub fn from_raw(bytes: &[u8]) -> Self {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        Self {
            encoded: BASE64.encode(bytes),
        }
    }

    /// Decode the base64 payload, accepting both padded and unpadded input.
    ///
    /// The PDS strips padding from `$bytes` fields during CBOR→JSON
    /// re-serialization, so we must be tolerant on decode.
    pub fn decode(&self) -> Result<Vec<u8>, Error> {
        use base64::engine::general_purpose::{GeneralPurpose, PAD};
        use base64::engine::DecodePaddingMode;
        use base64::{alphabet, Engine};

        const INDIFFERENT: GeneralPurpose = GeneralPurpose::new(
            &alphabet::STANDARD,
            PAD.with_decode_padding_mode(DecodePaddingMode::Indifferent),
        );

        INDIFFERENT
            .decode(&self.encoded)
            .map_err(|e| Error::InvalidEncoding(format!("invalid base64 in $bytes: {e}")))
    }
}
