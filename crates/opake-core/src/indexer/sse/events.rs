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

use crate::indexer::types::{classify_sse_envelope, EnvelopeClassification, IndexerEnvelope};
use crate::records::vocabulary::RecordKind;
use crate::records::{Directory, Document, Grant, Keyring, UnreadableReason, UnreadableRef};

// ---------------------------------------------------------------------------
// Delete payloads
// ---------------------------------------------------------------------------

/// Uri-only tombstone, used by directory/document/grant deletes.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SseDeletePayload {
    pub uri: String,
}

/// Resolved chain outcome of a keyring record delete. Only the indexer,
/// which holds the chain, can tell what a delete meant — clients act on
/// this resolution instead of re-deriving it from the deleted URI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyringDeleteOutcome {
    /// The chain head was deleted and rolled back to the newest live
    /// record. A `keyring:upsert` of the restored record follows on the
    /// same topics; clients rebuild through the ordinary upsert path.
    RolledBack,
    /// No live record remains in the chain — the workspace's keys are
    /// gone everywhere and its tracked state is removed.
    TornDown,
    /// The deleted record was not the chain head; tracked state is
    /// untouched. Deleting genesis or a superseded intermediate lands
    /// here — the genesis URI identifies the workspace, not a live
    /// record. Also the fail-safe under version skew: an absent or
    /// unrecognized outcome deserializes to this, so a client may
    /// under-react but never wrongly drop a living workspace.
    #[default]
    #[serde(other)]
    Unchanged,
}

/// Payload of `at.opake.keyring:delete`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SseKeyringDeletePayload {
    pub uri: String,
    /// Genesis keyring URI. Absent on legacy bare-URI payloads.
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub outcome: KeyringDeleteOutcome,
}

impl SseKeyringDeletePayload {
    /// The workspace identity this delete belongs to, falling back to
    /// the deleted URI when the payload predates the outcome contract.
    pub fn workspace_id(&self) -> &str {
        self.workspace_id.as_deref().unwrap_or(&self.uri)
    }
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
// Corrupt-record event — an upsert whose record body could not be understood.
// ---------------------------------------------------------------------------

/// Which keeper domain a corrupt SSE record belongs to. Derived from the wire
/// event name (a corrupt record body can't be trusted to say what it is), so a
/// keeper knows whether to placeholder it (directory/document → tree) or signal
/// it (keyring → workspace, grant → inbox).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CorruptScope {
    Directory,
    Document,
    Keyring,
    Grant,
}

/// A record delivered by SSE upsert that the client could not fully understand
/// — corrupt, or written by a newer schema version. Carries the envelope URI
/// (when extractable) so keepers can render a placeholder or signal an
/// unreadable workspace, exactly as the snapshot path does with an
/// [`UnreadableRef`]. The stream is never interrupted for one of these (see
/// `record-validity` § SSE delivery matches snapshot delivery).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SseCorruptRecord {
    /// AT-URI of the record, when the envelope yielded one; `None` is
    /// count-only (no placeholder can be hung on it).
    pub uri: Option<String>,
    /// Why the record is unreadable — corrupt vs needs-newer-client.
    pub reason: UnreadableReason,
    /// The keeper domain this record would have patched.
    pub scope: CorruptScope,
}

impl SseCorruptRecord {
    fn from_ref(reference: UnreadableRef, scope: CorruptScope) -> Self {
        Self {
            uri: reference.uri,
            reason: reference.reason,
            scope,
        }
    }
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
    KeyringDelete(SseKeyringDeletePayload),
    GrantUpsert(IndexerEnvelope<Grant>),
    GrantDelete(SseDeletePayload),

    /// An upsert whose record body was corrupt or future-version. Delivered as
    /// a normal event so keepers converge on the same state as the snapshot
    /// path — it never terminates the stream.
    CorruptRecord(SseCorruptRecord),

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
    "at.opake.directory:upsert",
    "at.opake.directory:delete",
    "at.opake.document:upsert",
    "at.opake.document:delete",
    "at.opake.keyring:upsert",
    "at.opake.keyring:delete",
    "at.opake.grant:upsert",
    "at.opake.grant:delete",
    "chain:forked",
];

impl SseEvent {
    /// The event type string as emitted by the broadcaster (fully-qualified
    /// collection identifiers — `at.opake.directory:upsert` etc.).
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::DirectoryUpsert(_) => "at.opake.directory:upsert",
            Self::DirectoryDelete(_) => "at.opake.directory:delete",
            Self::DocumentUpsert(_) => "at.opake.document:upsert",
            Self::DocumentDelete(_) => "at.opake.document:delete",
            Self::KeyringUpsert(_) => "at.opake.keyring:upsert",
            Self::KeyringDelete(_) => "at.opake.keyring:delete",
            Self::GrantUpsert(_) => "at.opake.grant:upsert",
            Self::GrantDelete(_) => "at.opake.grant:delete",
            Self::CorruptRecord(_) => "__corrupt__",
            Self::ChainForked(_) => "chain:forked",
            Self::Reconnect => "__reconnect__",
        }
    }

    /// Parse an event from its name + JSON data payload.
    ///
    /// Record upserts (directory/document/keyring/grant) NEVER return `Err`
    /// for a record-level failure: a corrupt or future-version record body
    /// yields `Ok(SseEvent::CorruptRecord)` so a poison record is a delivered
    /// event, not a stream fault (see `record-validity` § SSE delivery matches
    /// snapshot delivery). Only genuinely unparseable control payloads
    /// (deletes, `chain:forked`) or an unknown event name return `Err`; the
    /// connection/consumer layer treats those and true transport failures alike
    /// as the sole reconnect triggers.
    pub fn from_name_and_data(name: &str, data: &[u8]) -> Result<Self, crate::error::Error> {
        fn decode<T: serde::de::DeserializeOwned>(
            data: &[u8],
            tag: &str,
        ) -> Result<T, crate::error::Error> {
            serde_json::from_slice(data).map_err(|e| {
                crate::error::Error::Sse(format!("failed to parse {tag} payload: {e}"))
            })
        }

        /// Turn a lenient envelope classification into either the typed upsert
        /// (via `on_ok`) or a `CorruptRecord` event. Logs the skip so a poison
        /// record is never silent.
        fn upsert<T>(
            classification: EnvelopeClassification<T>,
            scope: CorruptScope,
            on_ok: impl FnOnce(IndexerEnvelope<T>) -> SseEvent,
        ) -> SseEvent {
            match classification {
                EnvelopeClassification::Understood(env) => on_ok(env),
                EnvelopeClassification::Unreadable(reference) => {
                    log::warn!(
                        "[sse] delivering unreadable {scope:?} record {:?} as corrupt event: {:?}",
                        reference.uri,
                        reference.reason,
                    );
                    SseEvent::CorruptRecord(SseCorruptRecord::from_ref(reference, scope))
                }
            }
        }

        let event = match name {
            "at.opake.directory:upsert" => upsert(
                classify_sse_envelope::<Directory>(RecordKind::Directory, data),
                CorruptScope::Directory,
                Self::DirectoryUpsert,
            ),
            "at.opake.directory:delete" => {
                Self::DirectoryDelete(decode(data, "at.opake.directory:delete")?)
            }
            "at.opake.document:upsert" => upsert(
                classify_sse_envelope::<Document>(RecordKind::Document, data),
                CorruptScope::Document,
                Self::DocumentUpsert,
            ),
            "at.opake.document:delete" => {
                Self::DocumentDelete(decode(data, "at.opake.document:delete")?)
            }
            "at.opake.keyring:upsert" => upsert(
                classify_sse_envelope::<Keyring>(RecordKind::Keyring, data),
                CorruptScope::Keyring,
                Self::KeyringUpsert,
            ),
            "at.opake.keyring:delete" => {
                Self::KeyringDelete(decode(data, "at.opake.keyring:delete")?)
            }
            "at.opake.grant:upsert" => upsert(
                classify_sse_envelope::<Grant>(RecordKind::Grant, data),
                CorruptScope::Grant,
                Self::GrantUpsert,
            ),
            "at.opake.grant:delete" => Self::GrantDelete(decode(data, "at.opake.grant:delete")?),
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
                .lineage
                .as_deref()
                .or(Some(env.uri.as_str())),
            Self::ChainForked(f) => Some(f.workspace_id.as_str()),
            Self::KeyringDelete(p) => Some(p.workspace_id()),
            Self::DirectoryDelete(_)
            | Self::DocumentDelete(_)
            | Self::GrantUpsert(_)
            | Self::GrantDelete(_)
            // A corrupt record body can't be trusted to name its workspace, and
            // the URI alone doesn't identify one — keepers place it by matching
            // its URI against references already in their authorized state.
            | Self::CorruptRecord(_)
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
            "workspace_id": "at://did:plc:alice/at.opake.keyring/kr1",
            "scope": "directory",
            "path": "/q1/",
            "your_uri": "at://did:plc:bob/at.opake.directory/loserTID",
            "fork_point_uri": "at://did:plc:alice/at.opake.directory/headTID",
            "winner_uri": "at://did:plc:carol/at.opake.directory/winnerTID",
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
            "workspace_id": "at://did:plc:alice/at.opake.keyring/kr1",
            "scope": "keyring",
            "your_uri": "at://did:plc:bob/at.opake.keyring/loserTID",
            "fork_point_uri": "at://did:plc:alice/at.opake.keyring/headTID",
            "winner_uri": "at://did:plc:carol/at.opake.keyring/winnerTID",
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
        let json = br#"{"uri": "at://did:plc:alice/at.opake.directory/abc"}"#;
        let event = SseEvent::from_name_and_data("at.opake.directory:delete", json).unwrap();
        match event {
            SseEvent::DirectoryDelete(p) => {
                assert_eq!(p.uri, "at://did:plc:alice/at.opake.directory/abc");
            }
            _ => panic!("expected DirectoryDelete"),
        }
    }

    #[test]
    fn decodes_keyring_delete_with_outcome() {
        for (wire, expected) in [
            ("unchanged", KeyringDeleteOutcome::Unchanged),
            ("rolled_back", KeyringDeleteOutcome::RolledBack),
            ("torn_down", KeyringDeleteOutcome::TornDown),
        ] {
            let json = format!(
                r#"{{"uri": "at://did:plc:alice/at.opake.keyring/head",
                     "workspace_id": "at://did:plc:alice/at.opake.keyring/genesis",
                     "outcome": "{wire}"}}"#
            );
            let event =
                SseEvent::from_name_and_data("at.opake.keyring:delete", json.as_bytes()).unwrap();
            match event {
                SseEvent::KeyringDelete(p) => {
                    assert_eq!(p.outcome, expected);
                    assert_eq!(
                        p.workspace_id(),
                        "at://did:plc:alice/at.opake.keyring/genesis"
                    );
                }
                _ => panic!("expected KeyringDelete"),
            }
        }
    }

    /// Version-skew fail-safe: a bare `{uri}` payload from an indexer
    /// that predates the outcome contract must default to `Unchanged`
    /// (never wrongly drop a workspace) and fall back to the deleted
    /// URI as the workspace identity.
    #[test]
    fn keyring_delete_without_outcome_defaults_to_unchanged() {
        let json = br#"{"uri": "at://did:plc:alice/at.opake.keyring/genesis"}"#;
        let event = SseEvent::from_name_and_data("at.opake.keyring:delete", json).unwrap();
        match event {
            SseEvent::KeyringDelete(p) => {
                assert_eq!(p.outcome, KeyringDeleteOutcome::Unchanged);
                assert_eq!(
                    p.workspace_id(),
                    "at://did:plc:alice/at.opake.keyring/genesis"
                );
            }
            _ => panic!("expected KeyringDelete"),
        }
    }

    #[test]
    fn keyring_delete_unknown_outcome_defaults_to_unchanged() {
        let json = br#"{"uri": "at://a", "workspace_id": "at://g", "outcome": "exploded"}"#;
        let event = SseEvent::from_name_and_data("at.opake.keyring:delete", json).unwrap();
        match event {
            SseEvent::KeyringDelete(p) => {
                assert_eq!(p.outcome, KeyringDeleteOutcome::Unchanged);
            }
            _ => panic!("expected KeyringDelete"),
        }
    }

    #[test]
    fn keyring_delete_workspace_id_surfaces_on_event() {
        let json = br#"{"uri": "at://a", "workspace_id": "at://g", "outcome": "torn_down"}"#;
        let event = SseEvent::from_name_and_data("at.opake.keyring:delete", json).unwrap();
        assert_eq!(event.workspace_id(), Some("at://g"));
    }

    // -----------------------------------------------------------------------
    // Per-event lenience (poison-record-resilience, task 3.3)
    // -----------------------------------------------------------------------

    fn dir_upsert_envelope(rkey: &str, record: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "uri": format!("at://did:plc:author/at.opake.directory/{rkey}"),
            "record": record,
            "indexedAt": "2026-03-01T00:00:01Z"
        }))
        .unwrap()
    }

    fn well_formed_directory() -> serde_json::Value {
        serde_json::json!({
            "opakeVersion": 1,
            "keyWrapping": {
                "$type": "at.opake.defs#directKeyWrapping",
                "keys": [{
                    "did": "did:plc:me",
                    "ciphertext": { "$bytes": "AAAA" },
                    "algo": "x25519-mlkem768-hkdf-a256kw-v2"
                }]
            },
            "encryptedMetadata": { "ciphertext": { "$bytes": "AAAA" }, "nonce": { "$bytes": "BBBB" } },
            "createdAt": "2026-03-01T00:00:00Z"
        })
    }

    /// A well-formed directory upsert parses to the typed event.
    #[test]
    fn well_formed_directory_upsert_parses() {
        let data = dir_upsert_envelope("good", well_formed_directory());
        let event = SseEvent::from_name_and_data("at.opake.directory:upsert", &data).unwrap();
        assert!(matches!(event, SseEvent::DirectoryUpsert(_)));
    }

    /// A corrupt directory record body NEVER errors — it becomes a delivered
    /// `CorruptRecord` event carrying the envelope URI, so the stream survives.
    #[test]
    fn corrupt_directory_upsert_is_delivered_not_errored() {
        let malformed = serde_json::json!({
            "opakeVersion": 1,
            "createdAt": "2026-03-01T00:00:00Z"
            // no keyWrapping / encryptedMetadata
        });
        let data = dir_upsert_envelope("bad", malformed);
        let event = SseEvent::from_name_and_data("at.opake.directory:upsert", &data).unwrap();
        match event {
            SseEvent::CorruptRecord(c) => {
                assert_eq!(c.scope, CorruptScope::Directory);
                assert_eq!(c.reason, UnreadableReason::Corrupt);
                assert_eq!(
                    c.uri.as_deref(),
                    Some("at://did:plc:author/at.opake.directory/bad")
                );
            }
            other => panic!("expected CorruptRecord, got {other:?}"),
        }
    }

    /// A future-version directory (floor present) becomes a needs-newer-client
    /// `CorruptRecord`, matching the snapshot path's classification.
    #[test]
    fn future_version_directory_upsert_is_needs_newer() {
        let mut future = well_formed_directory();
        future["opakeVersion"] = serde_json::json!(crate::records::SCHEMA_VERSION + 1);
        let data = dir_upsert_envelope("future", future);
        let event = SseEvent::from_name_and_data("at.opake.directory:upsert", &data).unwrap();
        match event {
            SseEvent::CorruptRecord(c) => {
                assert_eq!(c.reason, UnreadableReason::NeedsNewerClient);
                assert_eq!(c.scope, CorruptScope::Directory);
            }
            other => panic!("expected CorruptRecord, got {other:?}"),
        }
    }

    /// An envelope with no extractable URI is count-only: still a delivered
    /// `CorruptRecord`, never an error, but `uri` is `None`.
    #[test]
    fn envelopeless_upsert_is_count_only_corrupt() {
        let data = br#"{"record": 42}"#;
        let event = SseEvent::from_name_and_data("at.opake.directory:upsert", data).unwrap();
        match event {
            SseEvent::CorruptRecord(c) => {
                assert!(c.uri.is_none());
                assert_eq!(c.reason, UnreadableReason::Corrupt);
            }
            other => panic!("expected CorruptRecord, got {other:?}"),
        }
    }

    /// A corrupt keyring upsert carries the Keyring scope so the workspace
    /// keeper can signal it distinctly.
    #[test]
    fn corrupt_keyring_upsert_carries_keyring_scope() {
        let data = serde_json::to_vec(&serde_json::json!({
            "uri": "at://did:plc:author/at.opake.keyring/kr",
            "record": { "opakeVersion": 1, "createdAt": "2026-03-01T00:00:00Z" },
            "indexedAt": "2026-03-01T00:00:01Z"
        }))
        .unwrap();
        let event = SseEvent::from_name_and_data("at.opake.keyring:upsert", &data).unwrap();
        match event {
            SseEvent::CorruptRecord(c) => assert_eq!(c.scope, CorruptScope::Keyring),
            other => panic!("expected CorruptRecord, got {other:?}"),
        }
    }

    /// Totally unparseable JSON for a record upsert is still a delivered
    /// event, not a stream-terminating error.
    #[test]
    fn garbage_upsert_bytes_do_not_error() {
        let event = SseEvent::from_name_and_data("at.opake.grant:upsert", b"not json at all");
        assert!(matches!(event, Ok(SseEvent::CorruptRecord(_))));
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
