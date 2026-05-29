// SSE event types — envelope-shaped record events + the chain-forked
// notification. Envelopes carry the verbatim on-PDS record JSON inside
// a `record` field plus indexer-managed metadata (`indexedAt`,
// `deletedAt`) as siblings; the same `IndexerEnvelope<T>` shape is
// used on the HTTP side (`/cabinet/snapshot`, `/workspace/sync`,
// `/keyrings`, `/inbox`) so deserialization is symmetric across both
// transports.
//
// One serde struct per record type — the same `Directory`/`Document`/
// `Keyring`/`Grant` used everywhere else in the codebase. No parallel
// `Sse*Record` shadow types. The Pillar-1 invariant: the bytes the PDS
// signed are the bytes the indexer stores and broadcasts.

use serde::{Deserialize, Serialize};

use crate::indexer::types::IndexerEnvelope;
use crate::records::{Directory, Document, Grant, Keyring};

// ---------------------------------------------------------------------------
// Delete payload — uri-only tombstone
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SseDeletePayload {
    pub uri: String,
}

// ---------------------------------------------------------------------------
// Fork detection — emitted to the losing writer when concurrent supersedes
// race against the same chain head.
// ---------------------------------------------------------------------------

/// Notification that the recipient's most recent supersede lost a fork race.
///
/// Emitted only to the loser's personal SSE topic. The winner just becomes
/// the new head via the normal `directory:upsert` / `keyring:upsert` stream.
/// Clients act on this by replaying the original intent against the new
/// head and retrying (bounded by an exponential-backoff budget).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SseChainForked {
    /// Genesis keyring URI of the workspace.
    pub workspace_id: String,
    /// Which chain forked: `"directory"` or `"keyring"`.
    pub scope: String,
    /// Workspace-relative POSIX-style path for `directory` scope.
    /// Absent for `keyring`.
    #[serde(default)]
    pub path: Option<String>,
    /// AT-URI of the recipient's losing write.
    pub your_uri: String,
    /// AT-URI the loser tried to supersede.
    pub fork_point_uri: String,
    /// AT-URI of the supersede that won the race.
    pub winner_uri: String,
    /// CID of the winning record as the indexer observed it.
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
    DirectoryUpsert(IndexerEnvelope<Directory>),
    DirectoryDelete(SseDeletePayload),
    DocumentUpsert(IndexerEnvelope<Document>),
    DocumentDelete(SseDeletePayload),
    KeyringUpsert(IndexerEnvelope<Keyring>),
    KeyringDelete(SseDeletePayload),
    GrantUpsert(IndexerEnvelope<Grant>),
    GrantDelete(SseDeletePayload),

    /// Indexer detected a fork race against this client's latest supersede.
    ChainForked(SseChainForked),

    /// Synthetic: the consumer reconnected after a transport error.
    /// Subscribers should full-sync.
    Reconnect,
}

/// Every wire-event name the indexer can emit. Used by transports that
/// need to register listeners up front (browser `EventSource`) — every
/// entry MUST match a branch in [`SseEvent::from_name_and_data`].
///
/// `Reconnect` is intentionally absent — it's a synthetic event emitted
/// by the consumer, never by the server.
pub const ALL_WIRE_EVENT_NAMES: &[&str] = &[
    "app.opake.directory:upsert",
    "app.opake.directory:delete",
    "app.opake.document:upsert",
    "app.opake.document:delete",
    "app.opake.keyring:upsert",
    "app.opake.keyring:delete",
    "app.opake.grant:upsert",
    "app.opake.grant:delete",
    "chain:forked",
];

impl SseEvent {
    /// The event type string as emitted by the broadcaster (fully-qualified
    /// collection identifiers — `app.opake.directory:upsert` etc.).
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::DirectoryUpsert(_) => "app.opake.directory:upsert",
            Self::DirectoryDelete(_) => "app.opake.directory:delete",
            Self::DocumentUpsert(_) => "app.opake.document:upsert",
            Self::DocumentDelete(_) => "app.opake.document:delete",
            Self::KeyringUpsert(_) => "app.opake.keyring:upsert",
            Self::KeyringDelete(_) => "app.opake.keyring:delete",
            Self::GrantUpsert(_) => "app.opake.grant:upsert",
            Self::GrantDelete(_) => "app.opake.grant:delete",
            Self::ChainForked(_) => "chain:forked",
            Self::Reconnect => "__reconnect__",
        }
    }

    /// Parse an event from its name + JSON data payload.
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
            "app.opake.directory:upsert" => {
                Self::DirectoryUpsert(decode(data, "app.opake.directory:upsert")?)
            }
            "app.opake.directory:delete" => {
                Self::DirectoryDelete(decode(data, "app.opake.directory:delete")?)
            }
            "app.opake.document:upsert" => {
                Self::DocumentUpsert(decode(data, "app.opake.document:upsert")?)
            }
            "app.opake.document:delete" => {
                Self::DocumentDelete(decode(data, "app.opake.document:delete")?)
            }
            "app.opake.keyring:upsert" => {
                Self::KeyringUpsert(decode(data, "app.opake.keyring:upsert")?)
            }
            "app.opake.keyring:delete" => {
                Self::KeyringDelete(decode(data, "app.opake.keyring:delete")?)
            }
            "app.opake.grant:upsert" => {
                Self::GrantUpsert(decode(data, "app.opake.grant:upsert")?)
            }
            "app.opake.grant:delete" => {
                Self::GrantDelete(decode(data, "app.opake.grant:delete")?)
            }
            "chain:forked" => Self::ChainForked(decode(data, "chain:forked")?),
            other => {
                log::debug!("[sse] ignoring unknown event type: {other}");
                return Err(crate::error::Error::Sse(format!(
                    "unknown event type: {other}"
                )));
            }
        };

        Ok(event)
    }

    /// The workspace identity (genesis keyring URI) this event affects,
    /// if any. Keyring upserts fall back to the envelope `uri` when the
    /// record itself doesn't carry `workspaceId` (genesis case).
    pub fn workspace_id(&self) -> Option<&str> {
        match self {
            Self::DirectoryUpsert(env) => env.record.workspace_id.as_deref(),
            Self::DocumentUpsert(env) => env.record.workspace_id.as_deref(),
            Self::KeyringUpsert(env) => env
                .record
                .workspace_id
                .as_deref()
                .or(Some(env.uri.as_str())),
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
    fn decodes_delete_envelope() {
        let json = br#"{"uri": "at://did:plc:alice/app.opake.directory/abc"}"#;
        let event = SseEvent::from_name_and_data("app.opake.directory:delete", json).unwrap();
        match event {
            SseEvent::DirectoryDelete(p) => {
                assert_eq!(p.uri, "at://did:plc:alice/app.opake.directory/abc");
            }
            _ => panic!("expected DirectoryDelete"),
        }
    }

    #[test]
    fn unknown_event_type_errors() {
        let json = br#"{}"#;
        let err = SseEvent::from_name_and_data("weather:sunny", json).unwrap_err();
        assert!(matches!(err, crate::error::Error::Sse(_)));
    }

    #[test]
    fn reconnect_synthetic_name() {
        let event = SseEvent::Reconnect;
        assert_eq!(event.event_name(), "__reconnect__");
    }

    /// `ALL_WIRE_EVENT_NAMES` is used by the browser SSE transport to
    /// register `EventSource` listeners — one per name. If a name in
    /// that list isn't recognized by `from_name_and_data`, the listener
    /// fires but the payload is dropped. If a name `from_name_and_data`
    /// accepts is missing from the list, the listener is never
    /// registered and the event is silently lost. Either way: SSE
    /// events vanish at the JS↔WASM boundary with no log line. Keep
    /// the two sides linked.
    #[test]
    fn all_wire_event_names_are_decodable() {
        for name in ALL_WIRE_EVENT_NAMES {
            // We pass an empty JSON object: every variant will either
            // accept it (if all fields are optional) or fail with a
            // deserialization error. Both are fine — the test only
            // verifies that the *name* is recognized, not that the
            // payload validates. The one failure mode we're guarding
            // against is "unknown event type", which is a distinct
            // error message.
            let err = SseEvent::from_name_and_data(name, b"{}");
            if let Err(crate::error::Error::Sse(msg)) = &err {
                assert!(
                    !msg.contains("unknown event type"),
                    "ALL_WIRE_EVENT_NAMES contains {name:?} but from_name_and_data \
                     doesn't recognize it (got: {msg})",
                );
            }
        }
    }
}
