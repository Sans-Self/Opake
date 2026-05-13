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
///
/// Indexer-emitted `modified_at` is intentionally dropped here — the
/// pre-federation proposal-cleanup heuristics that consumed it are gone
/// and no caller reads it. Serde silently ignores unknown fields, so the
/// indexer can keep emitting without coordination.
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
// Fork detection — emitted to the losing writer when concurrent supersedes
// race against the same chain head.
// ---------------------------------------------------------------------------

/// Notification that the recipient's most recent supersede lost a fork race.
///
/// Emitted only to the loser's personal SSE topic. The winner just becomes
/// the new head via the normal `directory:upsert` / `keyring:upsert`
/// stream. Clients act on this by replaying the original intent against
/// the new head and retrying (bounded by an exponential-backoff budget).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SseChainForked {
    /// Genesis keyring URI of the workspace. Identifies the workspace
    /// independently of chain advancement.
    pub workspace_id: String,
    /// Which chain forked: `"directory"` or `"keyring"`.
    pub scope: String,
    /// Workspace-relative POSIX-style path for `directory` scope.
    /// Absent for `keyring` (a workspace has exactly one keyring chain).
    #[serde(default)]
    pub path: Option<String>,
    /// AT-URI of the recipient's losing write.
    pub your_uri: String,
    /// AT-URI the loser tried to supersede. Lets retry distinguish "I lost
    /// one race against this head" from "I'm already N supersedes behind"
    /// — the former retries cheaply, the latter needs a fresh sync first.
    pub fork_point_uri: String,
    /// AT-URI of the supersede that won the race and is the new head.
    pub winner_uri: String,
    /// CID of the winning record as the indexer observed it. Pins exactly
    /// what to replay against and detects races between fork emission and
    /// retry (winner itself superseded before the retry lands).
    pub winner_cid: String,
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

    /// Indexer detected a fork race against this client's latest supersede.
    /// Drives retry logic in the SDK; doesn't touch the tree.
    ChainForked(SseChainForked),

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
            Self::ChainForked(_) => "chain:forked",
            Self::Reconnect => "__reconnect__",
        }
    }

    /// Parse an event from its name + JSON data payload. Used by the parser
    /// after it has extracted the `event: name` and `data: { ... }` fields
    /// from the SSE frame.
    pub fn from_name_and_data(name: &str, data: &[u8]) -> Result<Self, crate::error::Error> {
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
            "chain:forked" => Self::ChainForked(decode(data, "chain:forked")?),
            other => {
                // Silent drop — the indexer may add event types we don't
                // understand yet, and forward-compat beats hard-failure.
                log::debug!("[sse] ignoring unknown event type: {other}");
                return Err(crate::error::Error::Sse(format!(
                    "unknown event type: {other}"
                )));
            }
        };

        Ok(event)
    }

    /// The workspace keyring URI this event affects, if any.
    ///
    /// Record events carry `keyring_uri` natively (for workspace-scoped
    /// writes). Chain-fork events carry `workspace_id` which is the genesis
    /// keyring URI. Delete payloads and the synthetic Reconnect never carry
    /// one.
    pub fn keyring_uri(&self) -> Option<&str> {
        match self {
            Self::DirectoryUpsert(r) => r.keyring_uri.as_deref(),
            Self::DocumentUpsert(r) => r.keyring_uri.as_deref(),
            Self::KeyringUpsert(r) => Some(r.uri.as_str()),
            Self::ChainForked(f) => Some(f.workspace_id.as_str()),
            Self::DirectoryDelete(_)
            | Self::DocumentDelete(_)
            | Self::KeyringDelete(_)
            | Self::GrantUpsert(_)
            | Self::GrantDelete(_)
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
    fn decodes_chain_forked_with_directory_scope() {
        let json = br#"{
            "workspace_id": "at://did:plc:alice/app.opake.keyring/kr1",
            "scope": "directory",
            "path": "/q1/",
            "your_uri": "at://did:plc:bob/app.opake.directory/loserTID",
            "fork_point_uri": "at://did:plc:alice/app.opake.directory/headTID",
            "winner_uri": "at://did:plc:carol/app.opake.directory/winnerTID",
            "winner_cid": "bafywinner"
        }"#;
        let event = SseEvent::from_name_and_data("chain:forked", json).unwrap();
        match event {
            SseEvent::ChainForked(f) => {
                assert_eq!(f.scope, "directory");
                assert_eq!(f.path.as_deref(), Some("/q1/"));
                assert!(f.your_uri.contains("loserTID"));
                assert!(f.fork_point_uri.contains("headTID"));
                assert!(f.winner_uri.contains("winnerTID"));
                assert_eq!(f.winner_cid, "bafywinner");
            }
            _ => panic!("expected ChainForked"),
        }
    }

    #[test]
    fn decodes_chain_forked_keyring_scope_without_path() {
        let json = br#"{
            "workspace_id": "at://did:plc:alice/app.opake.keyring/kr1",
            "scope": "keyring",
            "your_uri": "at://did:plc:bob/app.opake.keyring/loserTID",
            "fork_point_uri": "at://did:plc:alice/app.opake.keyring/headTID",
            "winner_uri": "at://did:plc:carol/app.opake.keyring/winnerTID",
            "winner_cid": "bafywinner"
        }"#;
        let event = SseEvent::from_name_and_data("chain:forked", json).unwrap();
        match event {
            SseEvent::ChainForked(f) => {
                assert_eq!(f.scope, "keyring");
                assert!(f.path.is_none());
            }
            _ => panic!("expected ChainForked"),
        }
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
    }
}
