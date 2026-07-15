//! Version-pinned registry vocabulary.
//!
//! Loaded from `lexicons/vocabulary.json` — the language-neutral single source
//! of truth shared with the Elixir indexer (see `openspec/specs/record-validity`).
//! Vocabulary is cumulative: a value permitted at version N is permitted at
//! every version ≥ N. A record that declares version N but uses a value not in
//! version N's cumulative set for its field is corrupt.
//!
//! Structural discriminants (union `$type` tags) are deliberately absent: a new
//! union variant is a structural change and takes the new-NSID path, not a
//! vocabulary bump.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use super::classify::{is_future_version, peek_version, UnreadableReason};

/// A vocabulary-bearing field. Each corresponds to a registry-string field on
/// one or more record types. Adding a variant is itself a schema-version bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VocabularyField {
    /// `wrappedKey.algo` — key-wrap algorithm (grants, directories, keyrings, pair responses).
    KeyWrapAlgo,
    /// Encryption-envelope `algo` — content cipher (documents, keyrings).
    ContentEncryptionAlgo,
    /// `at.opake.publicKey` algorithm identifiers (x25519 / ml-kem / ed25519).
    PublicKeyAlgo,
    /// `pairRequest.algo` (KEM suite) and `pairResponse.algo` (symmetric cipher).
    PairingAlgo,
    /// `keyringMember.role` — plaintext authorization vocabulary.
    KeyringMemberRole,
}

impl VocabularyField {
    /// The JSON key under which this field's per-version value sets live.
    const fn json_key(self) -> &'static str {
        match self {
            Self::KeyWrapAlgo => "keyWrapAlgo",
            Self::ContentEncryptionAlgo => "contentEncryptionAlgo",
            Self::PublicKeyAlgo => "publicKeyAlgo",
            Self::PairingAlgo => "pairingAlgo",
            Self::KeyringMemberRole => "keyringMemberRole",
        }
    }

    const ALL: [Self; 5] = [
        Self::KeyWrapAlgo,
        Self::ContentEncryptionAlgo,
        Self::PublicKeyAlgo,
        Self::PairingAlgo,
        Self::KeyringMemberRole,
    ];
}

#[derive(Debug, Deserialize)]
struct RawVocabulary {
    fields: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
}

/// Cumulative permitted values per field: for each field, the set of every
/// value pinned at any version ≤ the field's own version keys, indexed so a
/// lookup at version N unions all entries with key ≤ N.
struct Vocabulary {
    /// field → (version → values pinned AT that version). Cumulative lookup
    /// unions all entries with version key ≤ the queried version.
    by_field: BTreeMap<VocabularyField, BTreeMap<u32, Vec<String>>>,
}

impl Vocabulary {
    fn load() -> Self {
        // `include_str!` binds the artifact at compile time — the same bytes
        // the indexer loads at boot, so drift is a conformance-test failure,
        // never a silent divergence.
        const JSON: &str = include_str!("../../../../lexicons/vocabulary.json");
        let raw: RawVocabulary =
            serde_json::from_str(JSON).expect("lexicons/vocabulary.json is malformed");

        let mut by_field = BTreeMap::new();
        for field in VocabularyField::ALL {
            let entries = raw
                .fields
                .get(field.json_key())
                .unwrap_or_else(|| panic!("vocabulary.json missing field {}", field.json_key()));
            let mut per_version: BTreeMap<u32, Vec<String>> = BTreeMap::new();
            for (version_key, values) in entries {
                // Non-numeric keys (`$comment`) are metadata, skipped.
                let Ok(version) = version_key.parse::<u32>() else {
                    continue;
                };
                let values: Vec<String> = values
                    .as_array()
                    .unwrap_or_else(|| panic!("vocabulary {} v{version} is not an array", field.json_key()))
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .unwrap_or_else(|| panic!("vocabulary {} v{version} has a non-string value", field.json_key()))
                            .to_owned()
                    })
                    .collect();
                per_version.insert(version, values);
            }
            by_field.insert(field, per_version);
        }
        Self { by_field }
    }

    /// Is `value` permitted for `field` at schema `version`? Cumulative: any
    /// value pinned at a version ≤ `version` qualifies.
    fn permits(&self, field: VocabularyField, version: u32, value: &str) -> bool {
        self.by_field
            .get(&field)
            .into_iter()
            .flat_map(|per_version| per_version.range(..=version))
            .any(|(_, values)| values.iter().any(|v| v == value))
    }
}

impl PartialOrd for VocabularyField {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for VocabularyField {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.json_key().cmp(other.json_key())
    }
}

static VOCABULARY: LazyLock<Vocabulary> = LazyLock::new(Vocabulary::load);

/// Whether `value` is a permitted vocabulary value for `field` under a record
/// declaring schema `version`. A `false` result means the record is corrupt on
/// vocabulary grounds (see `record-validity`).
pub fn permits(field: VocabularyField, version: u32, value: &str) -> bool {
    VOCABULARY.permits(field, version, value)
}

// ---------------------------------------------------------------------------
// Classification helper
// ---------------------------------------------------------------------------
//
// The single fixed point every lenient read surface routes through to decide
// how one raw record value is handled (see `openspec/specs/record-validity`).
// The order is load-bearing: peek the version first, judge a future version by
// the required-field floor alone, and only give a known version the full
// structural + vocabulary judgment.

/// The record kinds a lenient read surface classifies. Each maps to a
/// required-field floor (the top-level required fields of the client's newest
/// known schema) and a set of vocabulary-bearing values to check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    Directory,
    Document,
    Grant,
    Keyring,
    PublicKey,
    PairRequest,
    PairResponse,
    PendingShare,
}

impl RecordKind {
    /// Top-level required field names of the client's newest known schema.
    ///
    /// Additive evolution guarantees every future-version record still carries
    /// every one of these — so their presence on a raw value (checked without a
    /// full typed parse) is the one structural judgment a client is entitled to
    /// make about a future-version record. Missing a field here is corrupt
    /// regardless of the declared version (the laundering guard).
    fn required_floor(self) -> &'static [&'static str] {
        match self {
            Self::Directory => &[
                "opakeVersion",
                "keyWrapping",
                "encryptedMetadata",
                "createdAt",
            ],
            Self::Document => &[
                "opakeVersion",
                "blob",
                "encryption",
                "encryptedMetadata",
                "createdAt",
            ],
            Self::Grant => &[
                "opakeVersion",
                "document",
                "recipient",
                "wrappedKey",
                "encryptedMetadata",
                "createdAt",
            ],
            Self::Keyring => &[
                "opakeVersion",
                "algo",
                "members",
                "encryptedMetadata",
                "createdAt",
            ],
            Self::PublicKey => &[
                "opakeVersion",
                "x25519PublicKey",
                "x25519Algo",
                "mlKemPublicKey",
                "mlKemAlgo",
                "createdAt",
            ],
            Self::PairRequest => &[
                "opakeVersion",
                "x25519EphemeralKey",
                "mlKemEphemeralKey",
                "algo",
                "createdAt",
            ],
            Self::PairResponse => &[
                "opakeVersion",
                "request",
                "wrappedKey",
                "ciphertext",
                "nonce",
                "algo",
                "createdAt",
            ],
            Self::PendingShare => &[
                "opakeVersion",
                "document",
                "recipient",
                "encryptedMetadata",
                "createdAt",
            ],
        }
    }

    /// Whether every required-floor field is present as a top-level key.
    fn floor_present(self, raw: &Value) -> bool {
        let Some(obj) = raw.as_object() else {
            return false;
        };
        self.required_floor().iter().all(|f| obj.contains_key(*f))
    }

    /// Whether every vocabulary-bearing value on this record is permitted at
    /// the declared (known) `version`. Runs only after a successful typed parse,
    /// so the fields it navigates are structurally sound; a `false` result means
    /// the record declares a registry value outside its version's cumulative
    /// vocabulary and is therefore corrupt.
    fn vocabulary_valid(self, version: u32, raw: &Value) -> bool {
        match self {
            Self::Grant => wrapped_key_algo_ok(raw.get("wrappedKey"), version),
            Self::PendingShare => true, // no vocabulary-bearing fields
            Self::Directory => key_wrapping_ok(raw.get("keyWrapping"), version),
            Self::Document => document_encryption_ok(raw.get("encryption"), version),
            Self::Keyring => {
                algo_ok(raw.get("algo"), VocabularyField::ContentEncryptionAlgo, version)
                    && members_ok(raw.get("members"), version)
                    && key_history_ok(raw.get("keyHistory"), version)
            }
            Self::PublicKey => {
                algo_ok(raw.get("x25519Algo"), VocabularyField::PublicKeyAlgo, version)
                    && algo_ok(raw.get("mlKemAlgo"), VocabularyField::PublicKeyAlgo, version)
                    // `signingAlgo` is optional; absent is fine, present must be pinned.
                    && raw.get("signingAlgo").is_none_or(|v| {
                        v.as_str()
                            .is_some_and(|s| permits(VocabularyField::PublicKeyAlgo, version, s))
                    })
            }
            Self::PairRequest => algo_ok(raw.get("algo"), VocabularyField::PairingAlgo, version),
            Self::PairResponse => {
                algo_ok(raw.get("algo"), VocabularyField::PairingAlgo, version)
                    && wrapped_key_algo_ok(raw.get("wrappedKey"), version)
            }
        }
    }
}

/// Whether an algorithm-identifier value is a string permitted for `field` at
/// `version`. A missing or non-string value is not permitted.
fn algo_ok(value: Option<&Value>, field: VocabularyField, version: u32) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|s| permits(field, version, s))
}

/// Whether a `wrappedKey`'s `algo` is a permitted key-wrap identifier.
fn wrapped_key_algo_ok(wrapped_key: Option<&Value>, version: u32) -> bool {
    algo_ok(
        wrapped_key.and_then(|wk| wk.get("algo")),
        VocabularyField::KeyWrapAlgo,
        version,
    )
}

/// Directory `keyWrapping` union: `directKeyWrapping` carries `keys[].algo`;
/// `keyringKeyWrapping` has no vocabulary-bearing field (a `$type` discriminant
/// selects the structure — discriminants are not vocabulary).
fn key_wrapping_ok(key_wrapping: Option<&Value>, version: u32) -> bool {
    let Some(kw) = key_wrapping else {
        return false;
    };
    match kw.get("keys").and_then(Value::as_array) {
        Some(keys) => keys
            .iter()
            .all(|k| wrapped_key_algo_ok(Some(k), version)),
        None => true,
    }
}

/// Document `encryption` union: `directEncryption` carries an `envelope` with a
/// content-cipher `algo` and wrapped `keys`; `keyringEncryption` carries a
/// content-cipher `algo` alongside a `keyringRef` (no key-wrap vocabulary).
fn document_encryption_ok(encryption: Option<&Value>, version: u32) -> bool {
    let Some(enc) = encryption else {
        return false;
    };
    if let Some(envelope) = enc.get("envelope") {
        let content_ok = algo_ok(
            envelope.get("algo"),
            VocabularyField::ContentEncryptionAlgo,
            version,
        );
        let keys_ok = envelope
            .get("keys")
            .and_then(Value::as_array)
            .is_none_or(|keys| keys.iter().all(|k| wrapped_key_algo_ok(Some(k), version)));
        return content_ok && keys_ok;
    }
    // keyringEncryption: content-cipher algo sits at the top of the variant.
    algo_ok(
        enc.get("algo"),
        VocabularyField::ContentEncryptionAlgo,
        version,
    )
}

/// Every keyring member's wrapped-key algo and role must be permitted.
fn members_ok(members: Option<&Value>, version: u32) -> bool {
    members
        .and_then(Value::as_array)
        .is_some_and(|arr| arr.iter().all(|m| member_ok(m, version)))
}

fn member_ok(member: &Value, version: u32) -> bool {
    wrapped_key_algo_ok(member.get("wrappedKey"), version)
        && algo_ok(
            member.get("role"),
            VocabularyField::KeyringMemberRole,
            version,
        )
}

/// Historical keyring rotations carry the same member vocabulary; a poisoned
/// value in `keyHistory` corrupts the record just as one in `members` does.
fn key_history_ok(key_history: Option<&Value>, version: u32) -> bool {
    match key_history.and_then(Value::as_array) {
        Some(entries) => entries
            .iter()
            .all(|entry| members_ok(entry.get("members"), version)),
        None => true,
    }
}

/// Classify one raw record value on a lenient read surface.
///
/// The version is peeked before any typed parse. A future-version record is
/// judged by the required-field floor alone — floor present ⇒
/// [`UnreadableReason::NeedsNewerClient`] (NOT parsed), floor missing ⇒
/// [`UnreadableReason::Corrupt`] (the laundering guard). A known-version record
/// gets the full judgment: a structural parse failure, a missing/mistyped
/// `opakeVersion`, or a vocabulary violation each yields
/// [`UnreadableReason::Corrupt`].
///
/// Returns `Ok(record)` only for a known, well-formed, vocabulary-valid record.
/// On `Err`, the caller attaches the envelope URI (this function has no access
/// to it). Listing callers that keep future-version records may re-parse them
/// under the known schema — additive evolution guarantees the floor fields
/// carry their known-version meaning.
pub fn classify_record<T: DeserializeOwned>(
    kind: RecordKind,
    raw: &Value,
) -> Result<T, UnreadableReason> {
    let Some(version) = peek_version(raw) else {
        return Err(UnreadableReason::Corrupt);
    };

    if is_future_version(version) {
        return if kind.floor_present(raw) {
            Err(UnreadableReason::NeedsNewerClient)
        } else {
            Err(UnreadableReason::Corrupt)
        };
    }

    let record: T =
        serde_json::from_value(raw.clone()).map_err(|_| UnreadableReason::Corrupt)?;

    if !kind.vocabulary_valid(version, raw) {
        return Err(UnreadableReason::Corrupt);
    }

    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::SCHEMA_VERSION;

    #[test]
    fn artifact_loads_and_covers_every_field() {
        // Forces the LazyLock and asserts every enum variant has an entry.
        for field in VocabularyField::ALL {
            assert!(
                VOCABULARY.by_field.contains_key(&field),
                "vocabulary.json missing {}",
                field.json_key()
            );
        }
    }

    #[test]
    fn v1_values_are_permitted_at_v1() {
        assert!(permits(
            VocabularyField::KeyWrapAlgo,
            1,
            "x25519-mlkem768-hkdf-a256kw-v2"
        ));
        assert!(permits(VocabularyField::ContentEncryptionAlgo, 1, "aes-256-gcm"));
        assert!(permits(VocabularyField::PublicKeyAlgo, 1, "ml-kem-768"));
        assert!(permits(VocabularyField::PairingAlgo, 1, "x25519-mlkem768"));
        assert!(permits(VocabularyField::KeyringMemberRole, 1, "manager"));
    }

    #[test]
    fn unknown_value_is_not_permitted() {
        assert!(!permits(VocabularyField::KeyWrapAlgo, 1, "rot13"));
        assert!(!permits(VocabularyField::KeyringMemberRole, 1, "superuser"));
    }

    #[test]
    fn vocabulary_is_cumulative() {
        // Every value valid at v1 must remain valid at any later version.
        assert!(permits(
            VocabularyField::KeyWrapAlgo,
            SCHEMA_VERSION + 5,
            "x25519-mlkem768-hkdf-a256kw-v2"
        ));
    }

    #[test]
    fn v1_current_algo_constants_are_in_the_table() {
        // Guards against the resolve.rs / crypto constants drifting out of the
        // shared table (the EXPECTED_*_ALGO anti-pattern this replaces).
        assert!(permits(
            VocabularyField::KeyWrapAlgo,
            SCHEMA_VERSION,
            opake_crypto::HYBRID_WRAP_ALGO
        ));
    }
}
