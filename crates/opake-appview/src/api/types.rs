use serde::Serialize;

use crate::db::grants::IndexedGrant;
use crate::db::keyrings::IndexedKeyringMember;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxResponse {
    pub grants: Vec<GrantItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantItem {
    pub uri: String,
    pub owner_did: String,
    pub document_uri: String,
    pub created_at: String,
}

impl From<&IndexedGrant> for GrantItem {
    fn from(g: &IndexedGrant) -> Self {
        Self {
            uri: g.uri.clone(),
            owner_did: g.owner_did.clone(),
            document_uri: g.document_uri.clone(),
            created_at: g.created_at.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyringsResponse {
    pub keyrings: Vec<KeyringItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyringItem {
    pub uri: String,
    pub owner_did: String,
    pub indexed_at: String,
}

impl From<&IndexedKeyringMember> for KeyringItem {
    fn from(m: &IndexedKeyringMember) -> Self {
        Self {
            uri: m.keyring_uri.clone(),
            owner_did: m.owner_did.clone(),
            indexed_at: m.indexed_at.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub error: String,
}
