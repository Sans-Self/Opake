// Client-side response types for the indexer JSON API.
//
// Every record-returning endpoint emits envelope-shaped JSON:
//
//     { "record": <verbatim PDS JSON>, "indexedAt": "...", "deletedAt": "..." }
//
// `IndexerEnvelope<T>` deserializes that shape into a strongly-typed
// inner record (one of `records::Directory`/`Document`/`Keyring`/`Grant`).
// The Rust crate uses the same struct family for both the PDS read path
// and the indexer read path — there are no parallel `Sse*Record` or
// `Tree*` shadow types.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use crate::records::vocabulary::{self, RecordKind};
use crate::records::{Directory, Document, Grant, Keyring, UnreadableRef};
use crate::workspace::WorkspaceId;

/// Indexer envelope. Carries the verbatim on-PDS record JSON plus
/// indexer-managed metadata as siblings. `T` is the record type
/// (typically a `records::*` struct).
///
/// The `uri` field lives at the envelope level rather than inside the
/// record because atproto records don't carry their own AT-URI — the
/// URI is identity metadata, not content. Keeping it on the envelope
/// preserves the Pillar-1 invariant that `record` is byte-identical to
/// what the PDS holds.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerEnvelope<T> {
    /// The record's AT-URI.
    pub uri: String,
    /// Byte-identical to what the PDS holds (modulo JSON key ordering).
    pub record: T,
    /// Server-side timestamp at which the indexer committed the record.
    pub indexed_at: String,
    /// Soft-delete tombstone; `None` for live records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
}

impl<T> IndexerEnvelope<T> {
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

impl IndexerEnvelope<Keyring> {
    /// The stable workspace identity this keyring event belongs to.
    ///
    /// Derived as `record.workspace_id` falling back to the envelope URI:
    /// a record without `workspace_id` is the genesis keyring, whose own
    /// URI *is* the workspace ID. The envelope URI alone is the chain
    /// head — never use it to key workspace-scoped state, or every event
    /// on a superseded chain silently misses (see `Keyring::wrap_anchor`,
    /// the crypto-side twin of this resolution).
    pub fn workspace_id(&self) -> WorkspaceId {
        WorkspaceId::from_resolved(self.record.wrap_anchor(&self.uri))
    }
}

// ---------------------------------------------------------------------------
// Lenient per-record classification
// ---------------------------------------------------------------------------
//
// Indexer responses are member-authored records the indexer relays verbatim, so
// any element may be a record this client cannot fully understand — corrupt, or
// written by a newer schema version. A strict `Vec<IndexerEnvelope<T>>` fails
// the whole response on one bad element (a workspace-scale DoS with an everyday
// trigger: version skew). These response types instead classify each element
// individually: understood records parse into the typed vec, and corrupt or
// future-version records surface as `UnreadableRef`s carrying their URI, never
// silently dropped (see `openspec/specs/record-validity`).

/// The envelope shape before its inner `record` is classified: the AT-URI and
/// indexer metadata survive even when the record body is unreadable, so a
/// corrupt element still yields a URI to report and placeholder-render.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEnvelope {
    uri: String,
    record: serde_json::Value,
    indexed_at: String,
    #[serde(default)]
    deleted_at: Option<String>,
}

/// Outcome of classifying a single envelope-shaped payload leniently. Either
/// the record was understood (typed envelope) or it could not be — corrupt or
/// future-version — in which case a URI-carrying reference is produced instead.
///
/// The classification itself never fails: a totally unparseable payload yields
/// an `Unreadable` with `uri: None` (count-only). This is what lets the SSE
/// path treat a poison record as a delivered event rather than a stream fault.
#[derive(Debug, Clone)]
pub enum EnvelopeClassification<T> {
    Understood(IndexerEnvelope<T>),
    Unreadable(UnreadableRef),
}

/// Classify a single envelope-shaped SSE `data` payload without ever failing.
///
/// The envelope is parsed as a raw `serde_json::Value` FIRST so the AT-URI and
/// indexer metadata survive even when the inner `record` is corrupt (the SSE
/// twin of `RawEnvelope`). The record body is then run through the shared
/// [`vocabulary::classify_record`] fixed point:
///
/// - understood record with a URI ⇒ [`EnvelopeClassification::Understood`]
/// - corrupt / future-version record ⇒ [`EnvelopeClassification::Unreadable`]
///   carrying the URI when one was extractable (count-only otherwise)
///
/// A payload that is not even a JSON object, or one missing `record`/`uri`,
/// still classifies as `Unreadable` — it is never a transport error. This is
/// the load-bearing half of the SSE parse-vs-transport split: only genuine
/// transport failures may terminate the stream (see `record-validity` § SSE
/// delivery matches snapshot delivery).
pub fn classify_sse_envelope<T: DeserializeOwned>(
    kind: RecordKind,
    data: &[u8],
) -> EnvelopeClassification<T> {
    let value: serde_json::Value = serde_json::from_slice(data).unwrap_or(serde_json::Value::Null);
    let uri = value
        .get("uri")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);

    let Some(record) = value.get("record") else {
        // No envelope to hang a record on — count-only corrupt reference.
        return EnvelopeClassification::Unreadable(UnreadableRef::corrupt(uri));
    };

    match vocabulary::classify_record::<T>(kind, record) {
        Ok(record) => match uri {
            Some(uri) => {
                let indexed_at = value
                    .get("indexedAt")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let deleted_at = value
                    .get("deletedAt")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
                EnvelopeClassification::Understood(IndexerEnvelope {
                    uri,
                    record,
                    indexed_at,
                    deleted_at,
                })
            }
            // Understood body but no URI to address it by — treat as count-only
            // corrupt: a live record with no identity can't be placed.
            None => EnvelopeClassification::Unreadable(UnreadableRef::corrupt(None)),
        },
        Err(reason) => EnvelopeClassification::Unreadable(UnreadableRef { uri, reason }),
    }
}

/// Classify a vec of raw envelopes into typed records plus a tally of the
/// unreadable ones. Understood records keep their envelope; corrupt and
/// future-version records become `UnreadableRef`s (appended to `unreadable`)
/// and are skipped from the typed vec with a warning log.
fn classify_envelopes<T: DeserializeOwned>(
    kind: RecordKind,
    raw: Vec<RawEnvelope>,
    unreadable: &mut Vec<UnreadableRef>,
) -> Vec<IndexerEnvelope<T>> {
    let mut records = Vec::with_capacity(raw.len());
    for env in raw {
        match vocabulary::classify_record::<T>(kind, &env.record) {
            Ok(record) => records.push(IndexerEnvelope {
                uri: env.uri,
                record,
                indexed_at: env.indexed_at,
                deleted_at: env.deleted_at,
            }),
            Err(reason) => {
                log::warn!(
                    "skipping unreadable {kind:?} record {}: {reason:?}",
                    env.uri
                );
                unreadable.push(UnreadableRef {
                    uri: Some(env.uri),
                    reason,
                });
            }
        }
    }
    records
}

// ---------------------------------------------------------------------------
// Inbox
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct InboxResponse {
    pub grants: Vec<IndexerEnvelope<Grant>>,
    pub cursor: Option<String>,
    /// Grant records skipped or locked during classification. Surfaced to the
    /// client, never merely logged.
    pub unreadable: Vec<UnreadableRef>,
}

impl<'de> Deserialize<'de> for InboxResponse {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            grants: Vec<RawEnvelope>,
            #[serde(default)]
            cursor: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let mut unreadable = Vec::new();
        let grants = classify_envelopes(RecordKind::Grant, raw.grants, &mut unreadable);
        Ok(Self {
            grants,
            cursor: raw.cursor,
            unreadable,
        })
    }
}

// ---------------------------------------------------------------------------
// Workspaces (member listing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WorkspacesResponse {
    pub workspaces: Vec<IndexerEnvelope<Keyring>>,
    /// Keyring records skipped or locked during classification. A corrupt
    /// keyring removes only its workspace from the list, signalled distinctly.
    pub unreadable: Vec<UnreadableRef>,
}

impl<'de> Deserialize<'de> for WorkspacesResponse {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            workspaces: Vec<RawEnvelope>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let mut unreadable = Vec::new();
        let workspaces = classify_envelopes(RecordKind::Keyring, raw.workspaces, &mut unreadable);
        Ok(Self {
            workspaces,
            unreadable,
        })
    }
}

// ---------------------------------------------------------------------------
// Tree sync — snapshots and deltas
// ---------------------------------------------------------------------------

/// Response from `/cabinet/snapshot`, `/cabinet/sync`, `/workspace/snapshot`,
/// `/workspace/sync`. Directories and documents are envelopes carrying the
/// verbatim PDS record JSON.
#[derive(Debug, Clone)]
pub struct TreeDelta {
    pub directories: Vec<IndexerEnvelope<Directory>>,
    pub documents: Vec<IndexerEnvelope<Document>>,
    pub server_time: String,
    /// Present only on the workspace endpoint.
    pub workspace_id: Option<String>,
    /// Directory and document records skipped or locked during classification.
    /// URIs carry the collection segment, so a consumer can tell a corrupt
    /// directory (placeholder-renderable) from a corrupt document.
    pub unreadable: Vec<UnreadableRef>,
}

impl<'de> Deserialize<'de> for TreeDelta {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // No `rename_all`: the tree endpoints emit `server_time` and
        // `workspace_id` in snake_case (see the indexer's tree_helpers.ex),
        // unlike the camelCase envelope fields. The nested `RawEnvelope` carries
        // its own camelCase rename for `indexedAt`/`deletedAt`.
        #[derive(Deserialize)]
        struct Raw {
            directories: Vec<RawEnvelope>,
            documents: Vec<RawEnvelope>,
            server_time: String,
            #[serde(default)]
            workspace_id: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let mut unreadable = Vec::new();
        let directories =
            classify_envelopes(RecordKind::Directory, raw.directories, &mut unreadable);
        let documents = classify_envelopes(RecordKind::Document, raw.documents, &mut unreadable);
        Ok(Self {
            directories,
            documents,
            server_time: raw.server_time,
            workspace_id: raw.workspace_id,
            unreadable,
        })
    }
}

impl TreeDelta {
    /// The server timestamp to pass as `since` on the next sync request.
    pub fn sync_cursor(&self) -> &str {
        &self.server_time
    }

    /// Local staleness marker for the cache.
    pub fn fetched_at_millis(&self) -> u64 {
        crate::client::time::unix_now_millis()
    }
}

// ---------------------------------------------------------------------------
// Chain heads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ChainHeadResponse {
    pub head_uri: String,
    pub head_cid: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceChainHeadResponse {
    pub workspace_id: String,
    #[serde(default)]
    pub keyring: Option<ChainHeadResponse>,
    pub root_directory: Option<ChainHeadResponse>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The tree endpoints emit `server_time` and `workspace_id` in snake_case
    /// (the indexer's tree_helpers.ex), while the envelope fields are camelCase.
    /// A `rename_all = "camelCase"` on the TreeDelta wrapper made the client
    /// look for `serverTime`/`workspaceId`, so every real cabinet/workspace
    /// snapshot failed to parse ("missing field serverTime") — invisible to a
    /// fixture that used the wrong key. Assert the on-the-wire snake_case shape.
    #[test]
    #[allow(non_snake_case)] // bug__ regression-naming convention
    fn bug__tree_snapshot_server_time_is_snake_case() {
        let json = r#"{
            "directories": [],
            "documents": [],
            "server_time": "2026-03-01T00:00:02Z",
            "workspace_id": "at://did:plc:owner/at.opake.keyring/genesis"
        }"#;
        let delta: TreeDelta = serde_json::from_str(json).expect("snake_case tree response parses");
        assert_eq!(delta.server_time, "2026-03-01T00:00:02Z");
        assert_eq!(
            delta.workspace_id.as_deref(),
            Some("at://did:plc:owner/at.opake.keyring/genesis")
        );
    }

    #[test]
    fn deserialize_inbox_envelope() {
        let json = r#"{
            "grants": [
                {
                    "uri": "at://did:plc:author/at.opake.grant/tid1",
                    "record": {
                        "opakeVersion": 1,
                        "document": "at://did:plc:author/at.opake.document/doc1",
                        "recipient": "did:plc:me",
                        "wrappedKey": {
                            "did": "did:plc:me",
                            "ciphertext": {"$bytes": "AAAA"},
                            "algo": "x25519-mlkem768-hkdf-a256kw-v2"
                        },
                        "encryptedMetadata": {
                            "ciphertext": {"$bytes": "AAAA"},
                            "nonce": {"$bytes": "BBBB"}
                        },
                        "createdAt": "2026-03-01T12:00:00Z"
                    },
                    "indexedAt": "2026-03-01T12:00:01Z"
                }
            ],
            "cursor": "next-page"
        }"#;

        let resp: InboxResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.grants.len(), 1);
        assert_eq!(resp.grants[0].record.recipient, "did:plc:me");
        assert_eq!(
            resp.grants[0].uri,
            "at://did:plc:author/at.opake.grant/tid1"
        );
        assert_eq!(resp.cursor.as_deref(), Some("next-page"));
    }

    #[test]
    fn deserialize_empty_inbox() {
        let json = r#"{"grants": []}"#;
        let resp: InboxResponse = serde_json::from_str(json).unwrap();
        assert!(resp.grants.is_empty());
        assert!(resp.cursor.is_none());
        assert!(resp.unreadable.is_empty());
    }

    // -----------------------------------------------------------------------
    // Lenient classification regressions (poison-record-resilience)
    // -----------------------------------------------------------------------

    use crate::records::UnreadableReason;
    use serde_json::{json, Value};

    const AUTHOR: &str = "did:plc:author";

    fn dir_uri(rkey: &str) -> String {
        format!("at://{AUTHOR}/at.opake.directory/{rkey}")
    }

    /// A well-formed v1 directory record value.
    fn well_formed_directory() -> Value {
        json!({
            "opakeVersion": 1,
            "keyWrapping": {
                "$type": "at.opake.defs#directKeyWrapping",
                "keys": [{
                    "did": "did:plc:me",
                    "ciphertext": { "$bytes": "AAAA" },
                    "algo": "x25519-mlkem768-hkdf-a256kw-v2"
                }]
            },
            "encryptedMetadata": {
                "ciphertext": { "$bytes": "AAAA" },
                "nonce": { "$bytes": "BBBB" }
            },
            "createdAt": "2026-03-01T00:00:00Z"
        })
    }

    /// Wrap directory record values into a `TreeDelta` snapshot body.
    fn tree_delta(directories: &[(&str, Value)]) -> TreeDelta {
        let dirs: Vec<Value> = directories
            .iter()
            .map(|(rkey, record)| {
                json!({
                    "uri": dir_uri(rkey),
                    "record": record,
                    "indexedAt": "2026-03-01T00:00:01Z"
                })
            })
            .collect();
        let body = json!({
            "directories": dirs,
            "documents": [],
            "server_time": "2026-03-01T00:00:02Z"
        });
        serde_json::from_value(body).expect("tree delta deserializes leniently")
    }

    /// A malformed directory (missing the crypto envelope) must not fail the
    /// whole snapshot: the well-formed records parse and render, the malformed
    /// one is skipped and reported as a corrupt reference carrying its URI.
    #[test]
    #[allow(non_snake_case)] // bug__ regression-naming convention
    fn bug__malformed_directory_bricks_snapshot() {
        let malformed = json!({
            "opakeVersion": 1,
            "createdAt": "2026-03-01T00:00:00Z"
            // no keyWrapping, no encryptedMetadata
        });

        let delta = tree_delta(&[
            ("good1", well_formed_directory()),
            ("bad", malformed),
            ("good2", well_formed_directory()),
        ]);

        assert_eq!(delta.directories.len(), 2, "well-formed records all parse");
        assert_eq!(delta.unreadable.len(), 1);
        assert!(delta.unreadable[0].is_corrupt());
        assert_eq!(
            delta.unreadable[0].uri.as_deref(),
            Some(dir_uri("bad").as_str()),
            "corrupt reference carries the record URI"
        );
    }

    /// A well-formed record whose declared version exceeds the client's and
    /// whose payload satisfies the required-field floor stays visible, marked
    /// needs-newer-client — not parsed into the typed vec.
    #[test]
    fn future_version_with_floor_is_kept_and_marked() {
        let mut future = well_formed_directory();
        future["opakeVersion"] = json!(crate::records::SCHEMA_VERSION + 1);

        let delta = tree_delta(&[("good", well_formed_directory()), ("future", future)]);

        assert_eq!(
            delta.directories.len(),
            1,
            "future record is not typed-parsed"
        );
        assert_eq!(delta.unreadable.len(), 1);
        assert_eq!(
            delta.unreadable[0].reason,
            UnreadableReason::NeedsNewerClient
        );
        assert_eq!(
            delta.unreadable[0].uri.as_deref(),
            Some(dir_uri("future").as_str())
        );
    }

    /// Laundering guard: a garbage record stamped with a high version but
    /// missing the required-field floor is corrupt, never needs-newer-client.
    #[test]
    fn laundered_future_version_is_corrupt() {
        let laundered = json!({
            "opakeVersion": 999,
            "createdAt": "2026-03-01T00:00:00Z"
            // floor missing: no keyWrapping / encryptedMetadata
        });

        let delta = tree_delta(&[("laundered", laundered)]);

        assert!(delta.directories.is_empty());
        assert_eq!(delta.unreadable.len(), 1);
        assert!(
            delta.unreadable[0].is_corrupt(),
            "a high version does not launder missing required fields into needs-newer"
        );
    }

    /// A known-version record using vocabulary outside its version's cumulative
    /// set (an unknown key-wrap algo) is corrupt — a rule-breaker, not a mystery.
    #[test]
    fn vocabulary_violation_is_corrupt() {
        let mut violating = well_formed_directory();
        violating["keyWrapping"]["keys"][0]["algo"] = json!("rot13");

        let delta = tree_delta(&[("violating", violating)]);

        assert!(delta.directories.is_empty());
        assert_eq!(delta.unreadable.len(), 1);
        assert!(delta.unreadable[0].is_corrupt());
    }
}
