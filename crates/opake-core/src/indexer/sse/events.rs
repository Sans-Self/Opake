// SSE event types. Mirrors the payload shapes emitted by
// `apps/indexer/lib/opake_indexer/sse/broadcaster.ex` and validated by the
// Zod schemas in the shipped SDK's `packages/opake-sdk/src/event-stream.ts`.
//
// Fields are serde-lenient — every field that the broadcaster marks with
// `maybe_put` must deserialize successfully when absent. The SDK Zod schemas
// use `.nullish()` for the same fields, and this file mirrors that contract
// one-for-one so asymmetric parsing failures between platforms never happen.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Record events — drive TreeKeeper::apply_record
// ---------------------------------------------------------------------------

/// A directory record event. Sent when the indexer commits a directory
/// create/update from the firehose.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SseDirectoryRecord {
    pub directory_uri: String,
    pub owner_did: String,
    #[serde(default)]
    pub entries: Vec<String>,
    #[serde(default)]
    pub encrypted_metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub key_wrapping: Option<serde_json::Value>,
    /// Workspace scope. Absent for personal cabinet directories.
    #[serde(default)]
    pub keyring_uri: Option<String>,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub indexed_at: Option<String>,
}

/// A document record event.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SseDocumentRecord {
    pub document_uri: String,
    pub owner_did: String,
    #[serde(default)]
    pub encrypted_metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub encryption: Option<serde_json::Value>,
    #[serde(default)]
    pub blob_ref: Option<serde_json::Value>,
    #[serde(default)]
    pub keyring_uri: Option<String>,
    #[serde(default)]
    pub rotation: Option<u64>,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub indexed_at: Option<String>,
}

/// A keyring (workspace) record event.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SseKeyringRecord {
    pub uri: String,
    pub owner_did: String,
    #[serde(default)]
    pub rotation: Option<u64>,
    #[serde(default)]
    pub member_entries: Vec<serde_json::Value>,
    #[serde(default)]
    pub encrypted_metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub indexed_at: Option<String>,
}

/// A grant record event.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SseGrantRecord {
    pub uri: String,
    pub owner_did: String,
    #[serde(default)]
    pub recipient_did: Option<String>,
    pub document_uri: String,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// Delete payload — most delete events only carry a URI. Different event
/// types populate different keys (`uri`, `directory_uri`, `document_uri`),
/// so all three are optional and `uri()` picks whichever is present.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SseDeletePayload {
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub directory_uri: Option<String>,
    #[serde(default)]
    pub document_uri: Option<String>,
}

impl SseDeletePayload {
    /// Return whichever URI field is populated, preferring the collection-
    /// specific one if both are present.
    pub fn best_uri(&self) -> Option<&str> {
        self.directory_uri
            .as_deref()
            .or(self.document_uri.as_deref())
            .or(self.uri.as_deref())
    }
}

// ---------------------------------------------------------------------------
// Proposal events — drive ProposalDebouncer (NOT the tree)
// ---------------------------------------------------------------------------

/// A directory update proposal from a workspace member.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SseDirectoryUpdate {
    pub uri: String,
    pub author_did: String,
    pub action_type: String,
    #[serde(default)]
    pub keyring_uri: Option<String>,
    #[serde(default)]
    pub directory_uri: Option<String>,
    #[serde(default)]
    pub entry_uri: Option<String>,
    #[serde(default)]
    pub encrypted_metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub source_directory_uri: Option<String>,
    #[serde(default)]
    pub target_directory_uri: Option<String>,
    #[serde(default)]
    pub parent_directory_uri: Option<String>,
}

/// A keyring update proposal.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SseKeyringUpdate {
    pub uri: String,
    pub author_did: String,
    pub action_type: String,
    #[serde(default)]
    pub keyring_uri: Option<String>,
    #[serde(default)]
    pub member_did: Option<String>,
    #[serde(default)]
    pub member_public_key: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub encrypted_metadata: Option<serde_json::Value>,
}

/// A document update proposal.
///
/// Note: the `app.opake.documentUpdate` lexicon itself has no
/// `keyring` field — the indexer's firehose consumer injects `keyring_uri` at
/// dispatch time by joining through the documents table. When the
/// join succeeds, the broadcaster routes on the workspace topic
/// (where owners subscribe); when it fails (cabinet documents or a
/// backfill ordering edge case), the event is dropped and the
/// owner's next `sync_workspace_by_uri` call picks up the proposal
/// from the DB.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SseDocumentUpdate {
    pub uri: String,
    pub document_uri: String,
    pub author_did: String,
    #[serde(default)]
    pub keyring_uri: Option<String>,
    #[serde(default)]
    pub supersedes_uri: Option<String>,
}

// ---------------------------------------------------------------------------
// Top-level event enum
// ---------------------------------------------------------------------------

/// All events that flow through an `SseConnection`. Parsed from the
/// `event: <name>\ndata: {json}\n\n` framing by `sse::parser`.
///
/// `Reconnect` is a synthetic event — the parser never produces one. The
/// [`SseConsumer`](super::SseConsumer) emits it when a previously-successful
/// connection recovers from an error. Subscribers treat it as "full sync the
/// world because we may have missed events."
#[derive(Debug, Clone)]
pub enum SseEvent {
    // Record events — drive TreeKeeper::apply_record
    DirectoryUpsert(SseDirectoryRecord),
    DirectoryDelete(SseDeletePayload),
    DocumentUpsert(SseDocumentRecord),
    DocumentDelete(SseDeletePayload),
    KeyringUpsert(SseKeyringRecord),
    KeyringDelete(SseDeletePayload),
    GrantUpsert(SseGrantRecord),
    GrantDelete(SseDeletePayload),

    // Proposal events — drive ProposalDebouncer, NOT the tree. A proposal
    // is a pending-but-not-applied change. Patching the tree with one would
    // show unapplied proposals as if they were live, then "jump" when the
    // owner applies them seconds later.
    DirectoryUpdateUpsert(SseDirectoryUpdate),
    DirectoryUpdateDelete(SseDeletePayload),
    KeyringUpdateUpsert(SseKeyringUpdate),
    KeyringUpdateDelete(SseDeletePayload),
    DocumentUpdateUpsert(SseDocumentUpdate),
    DocumentUpdateDelete(SseDeletePayload),

    // Synthetic
    Reconnect,
}

impl SseEvent {
    /// The event type string as emitted by the broadcaster. Used for
    /// dispatch and for test assertions.
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::DirectoryUpsert(_) => "directory:upsert",
            Self::DirectoryDelete(_) => "directory:delete",
            Self::DocumentUpsert(_) => "document:upsert",
            Self::DocumentDelete(_) => "document:delete",
            Self::KeyringUpsert(_) => "keyring:upsert",
            Self::KeyringDelete(_) => "keyring:delete",
            Self::GrantUpsert(_) => "grant:upsert",
            Self::GrantDelete(_) => "grant:delete",
            Self::DirectoryUpdateUpsert(_) => "directory_update:upsert",
            Self::DirectoryUpdateDelete(_) => "directory_update:delete",
            Self::KeyringUpdateUpsert(_) => "keyring_update:upsert",
            Self::KeyringUpdateDelete(_) => "keyring_update:delete",
            Self::DocumentUpdateUpsert(_) => "document_update:upsert",
            Self::DocumentUpdateDelete(_) => "document_update:delete",
            Self::Reconnect => "__reconnect__",
        }
    }

    /// Parse an event from its name + JSON data payload. Used by the parser
    /// after it has extracted the `event: name` and `data: { ... }` fields
    /// from the SSE frame.
    pub fn from_name_and_data(name: &str, data: &[u8]) -> Result<Self, crate::error::Error> {
        let parse = |tag: &str| -> Result<serde_json::Value, crate::error::Error> {
            serde_json::from_slice(data).map_err(|e| {
                crate::error::Error::Sse(format!("failed to parse {tag} payload: {e}"))
            })
        };

        // Helper to reduce boilerplate: parse JSON → strongly-typed variant.
        fn decode<T: serde::de::DeserializeOwned>(
            data: &[u8],
            tag: &str,
        ) -> Result<T, crate::error::Error> {
            serde_json::from_slice(data).map_err(|e| {
                crate::error::Error::Sse(format!("failed to parse {tag} payload: {e}"))
            })
        }

        let event = match name {
            "directory:upsert" => Self::DirectoryUpsert(decode(data, "directory:upsert")?),
            "directory:delete" => Self::DirectoryDelete(decode(data, "directory:delete")?),
            "document:upsert" => Self::DocumentUpsert(decode(data, "document:upsert")?),
            "document:delete" => Self::DocumentDelete(decode(data, "document:delete")?),
            "keyring:upsert" => Self::KeyringUpsert(decode(data, "keyring:upsert")?),
            "keyring:delete" => Self::KeyringDelete(decode(data, "keyring:delete")?),
            "grant:upsert" => Self::GrantUpsert(decode(data, "grant:upsert")?),
            "grant:delete" => Self::GrantDelete(decode(data, "grant:delete")?),
            "directory_update:upsert" => {
                Self::DirectoryUpdateUpsert(decode(data, "directory_update:upsert")?)
            }
            "directory_update:delete" => {
                Self::DirectoryUpdateDelete(decode(data, "directory_update:delete")?)
            }
            "keyring_update:upsert" => {
                Self::KeyringUpdateUpsert(decode(data, "keyring_update:upsert")?)
            }
            "keyring_update:delete" => {
                Self::KeyringUpdateDelete(decode(data, "keyring_update:delete")?)
            }
            "document_update:upsert" => {
                Self::DocumentUpdateUpsert(decode(data, "document_update:upsert")?)
            }
            "document_update:delete" => {
                Self::DocumentUpdateDelete(decode(data, "document_update:delete")?)
            }
            other => {
                // Silent drop — the indexer may add event types we don't
                // understand yet, and forward-compat beats hard-failure.
                log::debug!("[sse] ignoring unknown event type: {other}");
                let _ = parse; // satisfy unused-binding when all variants
                               // above use `decode` directly
                return Err(crate::error::Error::Sse(format!(
                    "unknown event type: {other}"
                )));
            }
        };

        Ok(event)
    }

    /// True if this is a proposal event (drives the debouncer, not the tree).
    pub fn is_proposal(&self) -> bool {
        matches!(
            self,
            Self::DirectoryUpdateUpsert(_)
                | Self::DirectoryUpdateDelete(_)
                | Self::KeyringUpdateUpsert(_)
                | Self::KeyringUpdateDelete(_)
                | Self::DocumentUpdateUpsert(_)
                | Self::DocumentUpdateDelete(_)
        )
    }

    /// The workspace keyring URI this event affects, if any.
    ///
    /// Record events carry `keyring_uri` natively (for workspace-scoped
    /// writes). Proposal upserts carry it when the lexicon includes a
    /// `keyring` field — `directoryUpdate` and `keyringUpdate` do,
    /// `documentUpdate` does not, so those always return None and the
    /// caller must decide how to route them. Delete payloads and
    /// control events never carry one.
    pub fn keyring_uri(&self) -> Option<&str> {
        match self {
            Self::DirectoryUpsert(r) => r.keyring_uri.as_deref(),
            Self::DocumentUpsert(r) => r.keyring_uri.as_deref(),
            Self::KeyringUpsert(r) => Some(r.uri.as_str()),
            Self::DirectoryUpdateUpsert(p) => p.keyring_uri.as_deref(),
            Self::KeyringUpdateUpsert(p) => p.keyring_uri.as_deref(),
            Self::DocumentUpdateUpsert(p) => p.keyring_uri.as_deref(),
            Self::DirectoryDelete(_)
            | Self::DocumentDelete(_)
            | Self::KeyringDelete(_)
            | Self::GrantUpsert(_)
            | Self::GrantDelete(_)
            | Self::DirectoryUpdateDelete(_)
            | Self::KeyringUpdateDelete(_)
            | Self::DocumentUpdateDelete(_)
            | Self::Reconnect => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_directory_upsert_with_all_fields() {
        let json = br#"{
            "directory_uri": "at://did:plc:alice/app.opake.directory/abc",
            "owner_did": "did:plc:alice",
            "entries": ["at://did:plc:alice/app.opake.document/xyz"],
            "encrypted_metadata": {"ciphertext": "...", "nonce": "..."},
            "key_wrapping": {"type": "direct"},
            "keyring_uri": "at://did:plc:alice/app.opake.keyring/kr1",
            "deleted_at": null,
            "indexed_at": "2026-04-11T12:00:00Z"
        }"#;
        let event = SseEvent::from_name_and_data("directory:upsert", json).unwrap();
        match event {
            SseEvent::DirectoryUpsert(d) => {
                assert_eq!(
                    d.directory_uri,
                    "at://did:plc:alice/app.opake.directory/abc"
                );
                assert_eq!(d.entries.len(), 1);
                assert_eq!(
                    d.keyring_uri.as_deref(),
                    Some("at://did:plc:alice/app.opake.keyring/kr1")
                );
                assert_eq!(d.indexed_at.as_deref(), Some("2026-04-11T12:00:00Z"));
            }
            _ => panic!("expected DirectoryUpsert"),
        }
    }

    #[test]
    fn decodes_directory_upsert_with_absent_optional_fields() {
        // Cabinet directory — no keyring_uri, no indexed_at (broadcaster omits both).
        let json = br#"{
            "directory_uri": "at://did:plc:alice/app.opake.directory/abc",
            "owner_did": "did:plc:alice",
            "entries": []
        }"#;
        let event = SseEvent::from_name_and_data("directory:upsert", json).unwrap();
        match event {
            SseEvent::DirectoryUpsert(d) => {
                assert_eq!(d.keyring_uri, None);
                assert_eq!(d.indexed_at, None);
                assert!(d.entries.is_empty());
            }
            _ => panic!("expected DirectoryUpsert"),
        }
    }

    #[test]
    fn decodes_delete_with_only_uri() {
        let json = br#"{"uri": "at://did:plc:alice/app.opake.keyring/kr1"}"#;
        let event = SseEvent::from_name_and_data("keyring:delete", json).unwrap();
        match event {
            SseEvent::KeyringDelete(d) => {
                assert_eq!(
                    d.best_uri(),
                    Some("at://did:plc:alice/app.opake.keyring/kr1")
                );
            }
            _ => panic!("expected KeyringDelete"),
        }
    }

    #[test]
    fn decodes_directory_delete_with_specific_key() {
        let json = br#"{"directory_uri": "at://did:plc:alice/app.opake.directory/abc"}"#;
        let event = SseEvent::from_name_and_data("directory:delete", json).unwrap();
        match event {
            SseEvent::DirectoryDelete(d) => {
                assert_eq!(
                    d.best_uri(),
                    Some("at://did:plc:alice/app.opake.directory/abc")
                );
            }
            _ => panic!("expected DirectoryDelete"),
        }
    }

    #[test]
    fn decodes_keyring_upsert_with_members() {
        let json = br#"{
            "uri": "at://did:plc:alice/app.opake.keyring/kr1",
            "owner_did": "did:plc:alice",
            "rotation": 3,
            "member_entries": [
                {"did": "did:plc:bob", "wrappedKey": "..."}
            ]
        }"#;
        let event = SseEvent::from_name_and_data("keyring:upsert", json).unwrap();
        match event {
            SseEvent::KeyringUpsert(k) => {
                assert_eq!(k.rotation, Some(3));
                assert_eq!(k.member_entries.len(), 1);
            }
            _ => panic!("expected KeyringUpsert"),
        }
    }

    #[test]
    fn decodes_directory_update_proposal() {
        let json = br#"{
            "uri": "at://did:plc:alice/app.opake.directoryUpdate/prop1",
            "author_did": "did:plc:bob",
            "action_type": "addEntry",
            "keyring_uri": "at://did:plc:alice/app.opake.keyring/kr1",
            "directory_uri": "at://did:plc:alice/app.opake.directory/abc",
            "entry_uri": "at://did:plc:bob/app.opake.document/xyz"
        }"#;
        let event = SseEvent::from_name_and_data("directory_update:upsert", json).unwrap();
        assert!(event.is_proposal());
        match event {
            SseEvent::DirectoryUpdateUpsert(p) => {
                assert_eq!(p.action_type, "addEntry");
                assert_eq!(
                    p.keyring_uri.as_deref(),
                    Some("at://did:plc:alice/app.opake.keyring/kr1")
                );
            }
            _ => panic!("expected DirectoryUpdateUpsert"),
        }
    }

    #[test]
    fn decodes_document_update_without_keyring_uri() {
        // document_update has no `keyring` field in the lexicon.
        let json = br#"{
            "uri": "at://did:plc:bob/app.opake.documentUpdate/upd1",
            "document_uri": "at://did:plc:alice/app.opake.document/doc1",
            "author_did": "did:plc:bob",
            "supersedes_uri": null
        }"#;
        let event = SseEvent::from_name_and_data("document_update:upsert", json).unwrap();
        assert!(event.is_proposal());
        match event {
            SseEvent::DocumentUpdateUpsert(p) => {
                assert_eq!(p.keyring_uri, None);
                assert_eq!(p.supersedes_uri, None);
            }
            _ => panic!("expected DocumentUpdateUpsert"),
        }
    }

    #[test]
    fn record_events_are_not_proposals() {
        let json = br#"{"directory_uri":"a","owner_did":"b","entries":[]}"#;
        let event = SseEvent::from_name_and_data("directory:upsert", json).unwrap();
        assert!(!event.is_proposal());
    }

    #[test]
    fn unknown_event_type_errors() {
        let json = br#"{}"#;
        let err = SseEvent::from_name_and_data("weather:sunny", json).unwrap_err();
        assert!(matches!(err, crate::error::Error::Sse(_)));
    }

    #[test]
    fn event_name_roundtrips() {
        let json = br#"{"uri":"a","owner_did":"b","document_uri":"c"}"#;
        let event = SseEvent::from_name_and_data("grant:upsert", json).unwrap();
        assert_eq!(event.event_name(), "grant:upsert");
    }

    #[test]
    fn reconnect_synthetic_name() {
        let event = SseEvent::Reconnect;
        assert_eq!(event.event_name(), "__reconnect__");
        assert!(!event.is_proposal());
    }
}
