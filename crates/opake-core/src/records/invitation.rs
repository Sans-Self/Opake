use serde::{Deserialize, Serialize};

use super::{default_version, SCHEMA_VERSION};

pub const INVITATION_COLLECTION: &str = "app.opake.invitation";

/// An invitation to join a workspace or accept a file share.
///
/// Created by the workspace owner / sharer. Contains a random token
/// that forms part of the shareable link.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Invitation {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    /// AT URI of the target resource (keyring URI or document URI).
    pub target: String,
    /// `"workspace"` or `"share"`.
    #[serde(rename = "type")]
    pub invitation_type: String,
    /// Role to assign on acceptance (workspace only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Random token embedded in the shareable link.
    pub token: String,
    /// Maximum number of times this invitation can be accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u32>,
    /// Current acceptance count.
    #[serde(default)]
    pub uses: u32,
    /// Expiry timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub created_at: String,
}

impl Invitation {
    pub fn workspace(target: String, role: &str, token: String, created_at: String) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            target,
            invitation_type: "workspace".to_string(),
            role: Some(role.to_string()),
            token,
            max_uses: None,
            uses: 0,
            expires_at: None,
            created_at,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at
            .as_ref()
            .is_some_and(|exp| exp.as_str() < self.created_at.as_str())
    }

    pub fn is_exhausted(&self) -> bool {
        self.max_uses.is_some_and(|max| self.uses >= max)
    }

    pub fn is_valid(&self) -> bool {
        !self.is_expired() && !self.is_exhausted()
    }
}
