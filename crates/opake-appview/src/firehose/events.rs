use serde::Deserialize;

/// Top-level Jetstream WebSocket message.
#[derive(Debug, Deserialize)]
pub struct JetstreamEvent {
    pub did: String,
    pub time_us: i64,
    pub kind: String,
    pub commit: Option<CommitEvent>,
}

/// A commit event within a Jetstream message.
#[derive(Debug, Deserialize)]
pub struct CommitEvent {
    pub operation: String,
    pub collection: String,
    pub rkey: String,
    /// Present for create/update, absent for delete.
    pub record: Option<serde_json::Value>,
}

/// Collections we index.
pub const COLLECTION_GRANT: &str = "app.opake.grant";
pub const COLLECTION_KEYRING: &str = "app.opake.keyring";

/// Parsed event ready for indexing.
#[derive(Debug)]
pub enum IndexableEvent {
    UpsertGrant {
        uri: String,
        owner_did: String,
        recipient_did: String,
        document_uri: String,
        permissions: Option<String>,
        note: Option<String>,
        created_at: String,
    },
    DeleteGrant {
        uri: String,
    },
    UpsertKeyring {
        uri: String,
        owner_did: String,
        name: String,
        member_dids: Vec<String>,
    },
    DeleteKeyring {
        uri: String,
    },
}

/// Try to parse a Jetstream JSON message into an indexable event.
/// Returns None for events we don't care about (wrong collection, identity events, etc).
pub fn parse_event(raw: &str) -> Option<(IndexableEvent, i64)> {
    let event: JetstreamEvent = serde_json::from_str(raw).ok()?;

    if event.kind != "commit" {
        return None;
    }

    let commit = event.commit.as_ref()?;
    let uri = format!("at://{}/{}/{}", event.did, commit.collection, commit.rkey);

    match (commit.collection.as_str(), commit.operation.as_str()) {
        (COLLECTION_GRANT, "create" | "update") => {
            let record = commit.record.as_ref()?;
            let grant: opake_core::records::Grant = serde_json::from_value(record.clone()).ok()?;
            Some((
                IndexableEvent::UpsertGrant {
                    uri,
                    owner_did: event.did,
                    recipient_did: grant.recipient,
                    document_uri: grant.document,
                    permissions: grant.permissions,
                    note: grant.note,
                    created_at: grant.created_at,
                },
                event.time_us,
            ))
        }
        (COLLECTION_GRANT, "delete") => Some((IndexableEvent::DeleteGrant { uri }, event.time_us)),
        (COLLECTION_KEYRING, "create" | "update") => {
            let record = commit.record.as_ref()?;
            let keyring: opake_core::records::Keyring =
                serde_json::from_value(record.clone()).ok()?;
            let member_dids: Vec<String> = keyring.members.iter().map(|m| m.did.clone()).collect();
            Some((
                IndexableEvent::UpsertKeyring {
                    uri,
                    owner_did: event.did,
                    name: keyring.name,
                    member_dids,
                },
                event.time_us,
            ))
        }
        (COLLECTION_KEYRING, "delete") => {
            Some((IndexableEvent::DeleteKeyring { uri }, event.time_us))
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
