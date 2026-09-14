use serde::{Deserialize, Serialize};

use super::SCHEMA_VERSION;
use crate::atproto::AtBytes;

pub const PUBLIC_KEY_COLLECTION: &str = "at.opake.publicKey";
pub const PUBLIC_KEY_RKEY: &str = "self";

/// Algorithm identifier for the classical encryption key.
pub const X25519_ALGO: &str = "x25519";

/// Algorithm identifier for the post-quantum encapsulation key. Pinned to
/// ML-KEM-768 per BSI TR-02102 / ANSSI guidance for hybrid PQ deployments.
pub const ML_KEM_ALGO: &str = "ml-kem-768";

/// Algorithm identifier for the Ed25519 signing key.
pub const ED25519_ALGO: &str = "ed25519";

/// Singleton public key record published on the user's PDS.
/// Uses rkey "self" (like app.bsky.actor.profile).
///
/// Carries both an X25519 public key and an ML-KEM-768 public encapsulation
/// key for the hybrid post-quantum KEM construction. Both are required.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyRecord {
    pub opake_version: u32,
    /// Raw X25519 public key (32 bytes).
    pub x25519_public_key: AtBytes,
    /// Algorithm identifier for the classical encryption key (always `"x25519"`).
    pub x25519_algo: String,
    /// Raw ML-KEM-768 public encapsulation key (1184 bytes).
    pub ml_kem_public_key: AtBytes,
    /// Algorithm identifier for the post-quantum key (always `"ml-kem-768"`).
    pub ml_kem_algo: String,
    /// Ed25519 signing public key for DID-scoped authentication.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key: Option<AtBytes>,
    /// Algorithm for the signing key (always `"ed25519"` when present).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_algo: Option<String>,
    /// Signature over the closed account-bound public-key transcript.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<AtBytes>,
    /// Algorithm for `signature` (always `"ed25519"` when present).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_algo: Option<String>,
    pub created_at: String,
}

impl PublicKeyRecord {
    /// Create a record with the required X25519 and ML-KEM-768 public keys.
    pub fn new(
        x25519_public_key_bytes: &[u8],
        ml_kem_public_key_bytes: &[u8],
        created_at: &str,
    ) -> Self {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        Self {
            opake_version: SCHEMA_VERSION,
            x25519_public_key: AtBytes {
                encoded: BASE64.encode(x25519_public_key_bytes),
            },
            x25519_algo: X25519_ALGO.into(),
            ml_kem_public_key: AtBytes {
                encoded: BASE64.encode(ml_kem_public_key_bytes),
            },
            ml_kem_algo: ML_KEM_ALGO.into(),
            signature: None,
            signature_algo: None,
            signing_key: None,
            signing_algo: None,
            created_at: created_at.into(),
        }
    }

    /// Create a record with encryption keys plus an Ed25519 signing key.
    pub fn with_signing_key(
        x25519_public_key_bytes: &[u8],
        ml_kem_public_key_bytes: &[u8],
        signing_key_bytes: &[u8],
        created_at: &str,
    ) -> Self {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        Self {
            signing_key: Some(AtBytes {
                encoded: BASE64.encode(signing_key_bytes),
            }),
            signing_algo: Some(ED25519_ALGO.into()),
            ..Self::new(x25519_public_key_bytes, ml_kem_public_key_bytes, created_at)
        }
    }
}

impl PublicKeyRecord {
    fn validate_signature_scheme(&self) -> Result<(), crate::error::Error> {
        match self.opake_version {
            // Keep this dispatch version-pinned rather than selecting a
            // scheme from the current client build. A later record version
            // may add a case without changing what v1 means.
            1 if self.signing_algo.as_deref() == Some(ED25519_ALGO) => Ok(()),
            1 => Err(crate::error::Error::InvalidRecord(
                "missing or unsupported signing algorithm".into(),
            )),
            _ => Err(crate::error::Error::InvalidRecord(
                "unsupported public-key signature version".into(),
            )),
        }
    }

    fn validated_encryption_fields(&self) -> Result<(Vec<u8>, Vec<u8>), crate::error::Error> {
        let x25519_public_key = self.x25519_public_key.decode()?;
        let ml_kem_public_key = self.ml_kem_public_key.decode()?;
        if x25519_public_key.len() != 32
            || ml_kem_public_key.len() != opake_crypto::ML_KEM_PK_LEN
            || self.x25519_algo != X25519_ALGO
            || self.ml_kem_algo != ML_KEM_ALGO
        {
            return Err(crate::error::Error::InvalidRecord(
                "invalid public-key encryption bundle".into(),
            ));
        }
        Ok((x25519_public_key, ml_kem_public_key))
    }

    /// Encode field values, never JSON bytes or unknown additional fields.
    pub fn signature_transcript(&self, did: &str) -> Result<Vec<u8>, crate::error::Error> {
        self.validate_signature_scheme()?;
        let (x25519_public_key, ml_kem_public_key) = self.validated_encryption_fields()?;
        let signing_key = self
            .signing_key
            .as_ref()
            .ok_or_else(|| crate::error::Error::InvalidRecord("missing signing key".into()))?
            .decode()?;
        if signing_key.len() != 32 {
            return Err(crate::error::Error::InvalidRecord(
                "invalid signing key length".into(),
            ));
        }
        Ok(opake_crypto::public_key_signature_transcript(
            self.opake_version,
            did,
            &opake_crypto::EncryptionKeyFields {
                x25519_public_key: &x25519_public_key,
                x25519_algo: &self.x25519_algo,
                ml_kem_public_key: &ml_kem_public_key,
                ml_kem_algo: &self.ml_kem_algo,
            },
            &signing_key,
            self.signing_algo.as_deref().expect("validated above"),
            &self.created_at,
        ))
    }

    /// Sign this record with the account identity; bind the public signing key
    /// before building the transcript.
    pub fn sign(
        &mut self,
        did: &str,
        signing_key: &opake_crypto::Ed25519SigningKey,
    ) -> Result<(), crate::error::Error> {
        use ed25519_dalek::Signer;
        self.signing_key = Some(AtBytes::from_raw(signing_key.verifying_key().as_bytes()));
        self.signing_algo = Some(ED25519_ALGO.into());
        let signature = signing_key.sign(&self.signature_transcript(did)?);
        self.signature = Some(AtBytes::from_raw(&signature.to_bytes()));
        self.signature_algo = Some(ED25519_ALGO.into());
        Ok(())
    }

    /// Verify against the DID document's anchor, never the record's own key.
    pub fn verify_signature(
        &self,
        did: &str,
        anchor: &[u8; 32],
    ) -> Result<(), crate::error::Error> {
        let invalid =
            || crate::error::Error::InvalidRecord("invalid account public-key signature".into());
        self.validate_signature_scheme()?;
        if self.signature_algo.as_deref() != Some(ED25519_ALGO) {
            return Err(invalid());
        }
        if self
            .signing_key
            .as_ref()
            .ok_or_else(invalid)?
            .decode()?
            .as_slice()
            != anchor
        {
            return Err(invalid());
        }
        let bytes = self.signature.as_ref().ok_or_else(invalid)?.decode()?;
        let signature =
            opake_crypto::Ed25519Signature::from_slice(&bytes).map_err(|_| invalid())?;
        let key = opake_crypto::Ed25519VerifyingKey::from_bytes(anchor).map_err(|_| invalid())?;
        key.verify_strict(&self.signature_transcript(did)?, &signature)
            .map_err(|_| invalid())
    }

    /// Compute relationship consent for this validated hybrid encryption bundle.
    pub fn unverified_key_approval(
        &self,
        relationship_version: u32,
        scope_uri: &str,
        did: &str,
    ) -> Result<[u8; 32], crate::error::Error> {
        match relationship_version {
            // Approval schemes are selected by the relationship record's
            // declaration, never by the public-key record or client build.
            1 => {}
            _ => {
                return Err(crate::error::Error::InvalidRecord(
                    "unsupported approval version".into(),
                ));
            }
        }
        let (x25519_public_key, ml_kem_public_key) = self.validated_encryption_fields()?;
        Ok(opake_crypto::unverified_key_approval(
            relationship_version,
            scope_uri,
            did,
            &opake_crypto::EncryptionKeyFields {
                x25519_public_key: &x25519_public_key,
                x25519_algo: &self.x25519_algo,
                ml_kem_public_key: &ml_kem_public_key,
                ml_kem_algo: &self.ml_kem_algo,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> PublicKeyRecord {
        PublicKeyRecord::new(&[7; 32], &[8; 1184], "2026-09-12T00:00:00Z")
    }
    #[test]
    fn unsigned_records_preserve_keys() {
        // This is the shape emitted before signature fields existed. It must
        // continue to deserialize without a compatibility branch.
        let raw = serde_json::json!({
            "opakeVersion": 1,
            "x25519PublicKey": AtBytes::from_raw(&[7; 32]),
            "x25519Algo": X25519_ALGO,
            "mlKemPublicKey": AtBytes::from_raw(&[8; 1184]),
            "mlKemAlgo": ML_KEM_ALGO,
            "createdAt": "2026-09-12T00:00:00Z",
        });
        let parsed: PublicKeyRecord = serde_json::from_value(raw).unwrap();
        assert!(parsed.signature.is_none());
        assert!(parsed.signature_algo.is_none());
        assert_eq!(parsed.x25519_public_key.decode().unwrap(), vec![7; 32]);
        assert_eq!(parsed.ml_kem_public_key.decode().unwrap(), vec![8; 1184]);
    }
    #[test]
    fn signature_binds_account_and_all_covered_fields() {
        let key = opake_crypto::Ed25519SigningKey::from_bytes(&[42; 32]);
        let mut record = record();
        record.sign("did:plc:alice", &key).unwrap();
        let anchor = key.verifying_key().to_bytes();
        record.verify_signature("did:plc:alice", &anchor).unwrap();
        assert!(record.verify_signature("did:plc:bob", &anchor).is_err());
        let raw = serde_json::to_value(&record).unwrap();
        for field in [
            "x25519Algo",
            "mlKemAlgo",
            "signingAlgo",
            "signatureAlgo",
            "createdAt",
        ] {
            let mut changed = raw.clone();
            changed[field] = "altered".into();
            let changed: PublicKeyRecord = serde_json::from_value(changed).unwrap();
            assert!(
                changed.verify_signature("did:plc:alice", &anchor).is_err(),
                "{field}"
            );
        }
        for field in [
            "x25519PublicKey",
            "mlKemPublicKey",
            "signingKey",
            "signature",
        ] {
            let mut changed = raw.clone();
            changed[field] = serde_json::to_value(AtBytes::from_raw(&[1; 32])).unwrap();
            let changed: PublicKeyRecord = serde_json::from_value(changed).unwrap();
            assert!(
                changed.verify_signature("did:plc:alice", &anchor).is_err(),
                "{field}"
            );
        }
        record.opake_version = 2;
        assert!(record.verify_signature("did:plc:alice", &anchor).is_err());
    }

    #[test]
    fn signatures_and_approvals_reject_malformed_declared_key_fields() {
        let signing_key = opake_crypto::Ed25519SigningKey::from_bytes(&[42; 32]);
        let mut signed = record();
        signed.sign("did:plc:alice", &signing_key).unwrap();
        let anchor = signing_key.verifying_key().to_bytes();

        let mut malformed_x25519 = signed.clone();
        malformed_x25519.x25519_public_key = AtBytes::from_raw(&[7; 31]);
        assert!(malformed_x25519
            .verify_signature("did:plc:alice", &anchor)
            .is_err());
        assert!(malformed_x25519
            .unverified_key_approval(1, "at://scope", "did:plc:alice")
            .is_err());

        let mut malformed_ml_kem = signed.clone();
        malformed_ml_kem.ml_kem_public_key = AtBytes::from_raw(&[8; 1183]);
        assert!(malformed_ml_kem
            .verify_signature("did:plc:alice", &anchor)
            .is_err());
        assert!(malformed_ml_kem
            .unverified_key_approval(1, "at://scope", "did:plc:alice")
            .is_err());

        let mut malformed_signing = signed;
        malformed_signing.signing_key = Some(AtBytes::from_raw(&[42; 31]));
        assert!(malformed_signing
            .verify_signature("did:plc:alice", &anchor)
            .is_err());
    }

    // Both verifiers are held to one accept/reject boundary: this asserts the
    // Rust half, `KeyFetcherTest` asserts the indexer's.
    #[test]
    fn strict_ed25519_matches_the_shared_adversarial_vectors() {
        const FIXTURE: &str =
            include_str!("../../../../tests/vectors/ed25519-signature-vectors.json");
        let fixture: serde_json::Value =
            serde_json::from_str(FIXTURE).expect("the shared Ed25519 vectors are malformed");

        let hex = |value: &serde_json::Value| -> Vec<u8> {
            let text = value.as_str().expect("vector fields are hex strings");
            text.as_bytes()
                .chunks(2)
                .map(|pair| {
                    u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16)
                        .expect("vector fields are hex strings")
                })
                .collect()
        };

        let vectors = fixture["vectors"].as_array().expect("vectors is a list");
        assert!(
            vectors.iter().any(|vector| vector["expected"] == "accept"),
            "the fixture must prove agreement on acceptance, not only on rejection"
        );

        for vector in vectors {
            let name = vector["name"].as_str().unwrap();
            let public_key: [u8; 32] = hex(&vector["publicKeyHex"])
                .try_into()
                .unwrap_or_else(|_| panic!("{name}: public key is 32 bytes"));
            let signature = hex(&vector["signatureHex"]);
            let message = hex(&vector["messageHex"]);

            let accepted = opake_crypto::Ed25519VerifyingKey::from_bytes(&public_key)
                .ok()
                .zip(opake_crypto::Ed25519Signature::from_slice(&signature).ok())
                .is_some_and(|(key, signature)| key.verify_strict(&message, &signature).is_ok());

            match vector["expected"].as_str() {
                Some("accept") => assert!(accepted, "{name} should verify"),
                Some("reject") => assert!(!accepted, "{name} should not verify"),
                other => panic!("{name}: unknown expectation {other:?}"),
            }
        }
    }

    #[test]
    fn strict_ed25519_rejects_every_shared_small_order_point() {
        const FIXTURE: &str =
            include_str!("../../../../tests/vectors/ed25519-signature-vectors.json");
        let fixture: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
        let points = fixture["smallOrderPointsHex"]
            .as_array()
            .expect("smallOrderPointsHex is a list");
        assert_eq!(points.len(), 14);

        // A small-order key paired with a small-order R and S = 0 verifies
        // under a permissive verifier for any transcript at all. Signing the
        // record with the point as its declared key must stay unforgeable.
        for point in points {
            let bytes: [u8; 32] = point
                .as_str()
                .unwrap()
                .as_bytes()
                .chunks(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>()
                .try_into()
                .expect("small-order points are 32 bytes");

            let mut forgery = record();
            forgery.signing_key = Some(AtBytes::from_raw(&bytes));
            forgery.signing_algo = Some(ED25519_ALGO.into());
            forgery.signature_algo = Some(ED25519_ALGO.into());
            let mut signature = [0u8; 64];
            signature[..32].copy_from_slice(&bytes);
            forgery.signature = Some(AtBytes::from_raw(&signature));

            assert!(
                forgery.verify_signature("did:plc:alice", &bytes).is_err(),
                "small-order point {} was accepted as a signing key",
                point.as_str().unwrap()
            );
        }
    }

    #[test]
    fn json_encoding_and_extensions_do_not_change_signature_or_approval() {
        let key = opake_crypto::Ed25519SigningKey::from_bytes(&[42; 32]);
        let mut record = record();
        record.sign("did:plc:alice", &key).unwrap();
        let approval = record
            .unverified_key_approval(1, "at://scope", "did:plc:alice")
            .unwrap();
        let mut raw = serde_json::to_value(&record).unwrap();
        raw["futureExtension"] = serde_json::json!({"ignored": true});
        for field in [
            "x25519PublicKey",
            "mlKemPublicKey",
            "signingKey",
            "signature",
        ] {
            let value = raw[field]["$bytes"]
                .as_str()
                .unwrap()
                .trim_end_matches('=')
                .to_owned();
            raw[field]["$bytes"] = value.into();
        }
        let mut parsed: PublicKeyRecord =
            serde_json::from_str(&serde_json::to_string_pretty(&raw).unwrap()).unwrap();
        parsed
            .verify_signature("did:plc:alice", &key.verifying_key().to_bytes())
            .unwrap();
        parsed.created_at = "new timestamp".into();
        assert_eq!(
            approval,
            parsed
                .unverified_key_approval(1, "at://scope", "did:plc:alice")
                .unwrap()
        );
        assert_ne!(
            approval,
            parsed
                .unverified_key_approval(1, "at://other", "did:plc:alice")
                .unwrap()
        );
        assert_ne!(
            approval,
            parsed
                .unverified_key_approval(1, "at://scope", "did:plc:bob")
                .unwrap()
        );
        let mut changed = parsed.clone();
        changed.x25519_public_key = AtBytes::from_raw(&[9; 32]);
        assert_ne!(
            approval,
            changed
                .unverified_key_approval(1, "at://scope", "did:plc:alice")
                .unwrap()
        );

        let mut changed = parsed.clone();
        changed.x25519_algo = "x25519-replacement".into();
        assert!(changed
            .unverified_key_approval(1, "at://scope", "did:plc:alice")
            .is_err());

        let mut changed = parsed.clone();
        changed.ml_kem_public_key = AtBytes::from_raw(&[9; 1184]);
        assert_ne!(
            approval,
            changed
                .unverified_key_approval(1, "at://scope", "did:plc:alice")
                .unwrap()
        );

        let mut changed = parsed.clone();
        changed.ml_kem_algo = "ml-kem-replacement".into();
        assert!(changed
            .unverified_key_approval(1, "at://scope", "did:plc:alice")
            .is_err());

        assert!(parsed
            .unverified_key_approval(2, "at://scope", "did:plc:alice")
            .is_err());
    }
}
