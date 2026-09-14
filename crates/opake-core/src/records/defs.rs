use std::fmt;

use serde::{Deserialize, Serialize};

use crate::atproto::AtBytes;

// `WrappedKey` and `EncryptedMetadata` are the literal output shapes of the
// wrap and metadata-encryption primitives in opake-crypto, re-exported here
// so record-level code can refer to them as `records::WrappedKey` etc.
pub use opake_crypto::{EncryptedMetadata, WrappedKey};

/// A member's role in a workspace (keyring).
///
/// Plaintext on the record because the Indexer needs it for authorization.
/// Grants ignore this field — it's only meaningful in keyring membership context.
///
/// The wire representation is an open string, not a closed union: an
/// unrecognized role value parses into [`Role::Unknown`] rather than failing
/// serde, so a single unknown role never bricks a whole response and whether
/// the value is *understood* is decided against the version-pinned vocabulary
/// table (`vocabulary::permits(KeyringMemberRole, …)`), not the type system
/// (see `record-validity` § schema evolution is additive and vocabulary is
/// version-pinned). Authorization is fail-closed: an `Unknown` role matches
/// none of the privileged arms, so it grants nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    Manager,
    Editor,
    Viewer,
    /// A role string this client does not recognize. Retained verbatim so the
    /// read path can classify it against the vocabulary table and so the value
    /// survives round-trips. Never treated as a privileged role.
    Unknown(String),
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manager => write!(f, "manager"),
            Self::Editor => write!(f, "editor"),
            Self::Viewer => write!(f, "viewer"),
            Self::Unknown(other) => write!(f, "{other}"),
        }
    }
}

impl Role {
    /// Map a wire string to a `Role`, retaining unrecognized values as
    /// [`Role::Unknown`]. This is the total, non-failing counterpart to
    /// [`FromStr`], used by deserialization so unknown vocabulary parses.
    pub fn from_wire(s: &str) -> Self {
        match s {
            "manager" => Self::Manager,
            "editor" => Self::Editor,
            "viewer" => Self::Viewer,
            other => Self::Unknown(other.to_owned()),
        }
    }
}

impl std::str::FromStr for Role {
    type Err = String;

    /// Strict parse for the three known roles. Unknown values are an error —
    /// this preserves the historical `FromStr` contract for callers that want
    /// rejection (e.g. validating user-supplied input). Wire deserialization
    /// uses [`Role::from_wire`] instead, which retains unknown values.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "manager" => Ok(Self::Manager),
            "editor" => Ok(Self::Editor),
            "viewer" => Ok(Self::Viewer),
            other => Err(format!(
                "unknown role: {other:?} (expected manager, editor, or viewer)"
            )),
        }
    }
}

impl Serialize for Role {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Role {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Role::from_wire(&s))
    }
}

/// An admitted member, independent of availability of its current key wrap.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyringMember {
    pub did: String,
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapped_key: Option<WrappedKey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unverified_key_approval: Option<AtBytes>,
}

impl KeyringMember {
    pub fn did(&self) -> &str {
        &self.did
    }

    /// Construct a newly wrapped entry. This never infers approval.
    pub fn with_wrap(wrapped_key: WrappedKey, role: Role) -> Self {
        Self {
            did: wrapped_key.did.clone(),
            role,
            wrapped_key: Some(wrapped_key),
            unverified_key_approval: None,
        }
    }
}

impl<'de> Deserialize<'de> for KeyringMember {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            did: String,
            role: Role,
            wrapped_key: Option<WrappedKey>,
            unverified_key_approval: Option<AtBytes>,
        }
        let wire = Wire::deserialize(deserializer)?;
        if !crate::atproto::is_valid_did(&wire.did) {
            return Err(serde::de::Error::custom("invalid member DID"));
        }
        if wire
            .wrapped_key
            .as_ref()
            .is_some_and(|wrap| wrap.did != wire.did)
        {
            return Err(serde::de::Error::custom("member wrap DID mismatch"));
        }
        if wire
            .unverified_key_approval
            .as_ref()
            .is_some_and(|approval| approval.decode().map_or(true, |bytes| bytes.len() != 32))
        {
            return Err(serde::de::Error::custom(
                "member approval must contain 32 bytes",
            ));
        }
        Ok(Self {
            did: wire.did,
            role: wire.role,
            wrapped_key: wire.wrapped_key,
            unverified_key_approval: wire.unverified_key_approval,
        })
    }
}

pub(super) fn deserialize_members<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<KeyringMember>, D::Error> {
    let members = Vec::<KeyringMember>::deserialize(deserializer)?;
    let mut seen = std::collections::HashSet::new();
    if members.iter().any(|member| !seen.insert(&member.did)) {
        return Err(serde::de::Error::custom("duplicate member DID"));
    }
    Ok(members)
}

/// Describes how a blob's content was symmetrically encrypted, plus one or
/// more wrapped copies of the content key for authorized DIDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionEnvelope {
    pub algo: String,
    pub nonce: AtBytes,
    pub keys: Vec<WrappedKey>,
}

/// Reference to a keyring whose group key protects the content key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyringRef {
    pub keyring: String,
    pub wrapped_content_key: AtBytes,
    pub rotation: u64,
}

// ---------------------------------------------------------------------------
// Key wrapping for records without blobs (directories, grants)
// ---------------------------------------------------------------------------

/// Content key wrapped directly to individual DIDs.
///
/// Unlike `DirectEncryption` (for documents), this only carries the key
/// material — no blob-specific `algo` or `nonce` fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectKeyWrapping {
    pub keys: Vec<WrappedKey>,
}

/// Content key wrapped under a keyring's group key.
///
/// Unlike `KeyringEncryption` (for documents), this only carries the keyring
/// reference — no blob-specific `algo` or `nonce` fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyringKeyWrapping {
    pub keyring_ref: KeyringRef,
}

/// How to unwrap the content key for a record that has no blob.
///
/// Used by directories (and in future, grants). Documents use `Encryption`
/// instead, which adds blob-specific fields (`algo`, `nonce`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "$type")]
pub enum KeyWrapping {
    #[serde(rename = "at.opake.defs#directKeyWrapping")]
    Direct(DirectKeyWrapping),
    #[serde(rename = "at.opake.defs#keyringKeyWrapping")]
    Keyring(KeyringKeyWrapping),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::vocabulary::{self, VocabularyField};
    use crate::records::SCHEMA_VERSION;
    use std::str::FromStr;

    // record-validity § schema evolution is additive and vocabulary is version-pinned
    //
    // The role field is an OPEN string on the wire: an unrecognized value
    // parses into `Role::Unknown` rather than failing serde, so one unknown
    // role never bricks a whole response. Whether the value is *understood* is
    // a vocabulary question, decided against the version-pinned table — not the
    // type system.
    #[test]
    fn unknown_role_parses_instead_of_failing_serde() {
        let role: Role = serde_json::from_value(serde_json::json!("superuser")).unwrap();
        assert_eq!(role, Role::Unknown("superuser".to_owned()));
        // And understanding is decided by the vocabulary table, which does NOT
        // permit it at the current version.
        assert!(!vocabulary::permits(
            VocabularyField::KeyringMemberRole,
            SCHEMA_VERSION,
            "superuser"
        ));
    }

    #[test]
    fn known_roles_roundtrip_through_json() {
        for (role, wire) in [
            (Role::Manager, "manager"),
            (Role::Editor, "editor"),
            (Role::Viewer, "viewer"),
        ] {
            let json = serde_json::to_value(&role).unwrap();
            assert_eq!(json, serde_json::json!(wire));
            let parsed: Role = serde_json::from_value(json).unwrap();
            assert_eq!(parsed, role);
            // Known roles are permitted vocabulary.
            assert!(vocabulary::permits(
                VocabularyField::KeyringMemberRole,
                SCHEMA_VERSION,
                wire
            ));
        }
    }

    #[test]
    fn unknown_role_roundtrips_verbatim() {
        let role = Role::Unknown("auditor".to_owned());
        let json = serde_json::to_value(&role).unwrap();
        assert_eq!(json, serde_json::json!("auditor"));
        assert_eq!(role.to_string(), "auditor");
    }

    #[test]
    fn from_str_stays_strict_for_known_roles() {
        // The historical `FromStr` contract is preserved: known roles parse,
        // unknown values are an error (used for validating user input, not wire
        // deserialization).
        assert_eq!(Role::from_str("manager").unwrap(), Role::Manager);
        assert_eq!(Role::from_str("editor").unwrap(), Role::Editor);
        assert_eq!(Role::from_str("viewer").unwrap(), Role::Viewer);
        assert!(Role::from_str("superuser").is_err());
    }

    #[test]
    fn unknown_role_grants_no_authority() {
        // Fail-closed: an unknown role matches none of the privileged arms.
        let role = Role::Unknown("manager-plus".to_owned());
        assert!(!matches!(role, Role::Manager));
    }

    fn member_json() -> serde_json::Value {
        serde_json::json!({
            "did": "did:plc:alice",
            "role": "editor",
        })
    }

    #[test]
    fn member_allows_missing_current_wrap() {
        let member: KeyringMember = serde_json::from_value(member_json()).unwrap();
        assert_eq!(member.did(), "did:plc:alice");
        assert!(member.wrapped_key.is_none());
        assert!(member.unverified_key_approval.is_none());
    }

    #[test]
    fn member_rejects_prior_draft_and_mismatched_wrap() {
        let prior_draft = serde_json::json!({
            "wrappedKey": {"did": "did:plc:alice", "ciphertext": {"$bytes": "AAAA"}, "algo": "x25519-mlkem768-hkdf-a256kw-v2"},
            "role": "editor",
        });
        assert!(serde_json::from_value::<KeyringMember>(prior_draft).is_err());

        let mut mismatched = member_json();
        mismatched["wrappedKey"] = serde_json::json!({
            "did": "did:plc:bob", "ciphertext": {"$bytes": "AAAA"}, "algo": "x25519-mlkem768-hkdf-a256kw-v2"
        });
        assert!(serde_json::from_value::<KeyringMember>(mismatched).is_err());
    }

    #[test]
    fn member_did_requires_a_well_formed_did_core_identifier() {
        for did in [
            "did:plc:alice",
            "did:web:example.com",
            "did:peer:2.Ez6LSfoo:service",
            "did:example:abc%3Adef",
        ] {
            let mut member = member_json();
            member["did"] = serde_json::json!(did);
            assert!(
                serde_json::from_value::<KeyringMember>(member).is_ok(),
                "{did}"
            );
        }

        for did in [
            "did:PLC:alice",
            "did::alice",
            "did:plc:",
            "did:plc::alice",
            "did:plc:alice smith",
            "did:plc:alice%Q1",
        ] {
            let mut member = member_json();
            member["did"] = serde_json::json!(did);
            assert!(
                serde_json::from_value::<KeyringMember>(member).is_err(),
                "{did}"
            );
        }
    }

    #[test]
    fn member_rejects_malformed_approval_and_duplicate_did() {
        let mut malformed = member_json();
        malformed["unverifiedKeyApproval"] = serde_json::json!({"$bytes": "AAAA"});
        assert!(serde_json::from_value::<KeyringMember>(malformed).is_err());

        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct Members(
            #[serde(deserialize_with = "super::deserialize_members")] Vec<KeyringMember>,
        );
        let duplicate = serde_json::json!([member_json(), member_json()]);
        assert!(serde_json::from_value::<Members>(duplicate).is_err());
    }
}
