// Client-side response types for the appview JSON API.
//
// These mirror the appview's server-side types but only carry Deserialize —
// this crate doesn't need to serialize them.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxGrant {
    pub uri: String,
    pub owner_did: String,
    pub document_uri: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InboxResponse {
    pub grants: Vec<InboxGrant>,
    pub cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_response() {
        let json = r#"{
            "grants": [{
                "uri": "at://did:plc:owner/app.opake.grant/tid1",
                "ownerDid": "did:plc:owner",
                "documentUri": "at://did:plc:owner/app.opake.document/doc1",
                "createdAt": "2026-03-01T12:00:00Z"
            }],
            "cursor": "next-page"
        }"#;

        let resp: InboxResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.grants.len(), 1);
        assert_eq!(resp.grants[0].owner_did, "did:plc:owner");
        assert_eq!(
            resp.grants[0].document_uri,
            "at://did:plc:owner/app.opake.document/doc1"
        );
        assert_eq!(resp.cursor.as_deref(), Some("next-page"));
    }

    #[test]
    fn deserialize_empty_response() {
        let json = r#"{"grants": []}"#;
        let resp: InboxResponse = serde_json::from_str(json).unwrap();
        assert!(resp.grants.is_empty());
        assert!(resp.cursor.is_none());
    }
}
