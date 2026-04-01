use serde::{Deserialize, Serialize};

use super::{default_version, AtBytes, EncryptedMetadata, SCHEMA_VERSION};

pub const KEYRING_UPDATE_COLLECTION: &str = "app.opake.keyringUpdate";

/// Action type strings for matching AppView proposal responses.
pub const ACTION_RENAME: &str = "rename";
pub const ACTION_UPDATE_DESCRIPTION: &str = "updateDescription";
pub const ACTION_ADD_MEMBER: &str = "addMember";
pub const ACTION_REMOVE_MEMBER: &str = "removeMember";
pub const ACTION_UPDATE_ROLE: &str = "updateRole";
pub const ACTION_LEAVE: &str = "leave";

/// A proposed change to a workspace keyring, with schema version envelope.
///
/// Written by a member to their own PDS. The owner's daemon picks up
/// pending updates via the AppView and applies them. `#[serde(flatten)]`
/// inlines the variant fields alongside `opakeVersion` on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyringUpdateRecord {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    #[serde(flatten)]
    pub update: KeyringUpdate,
}

/// The actual keyring update, discriminated by `actionType`.
///
/// Each variant carries exactly the fields it needs — no Option soup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "actionType", rename_all = "camelCase")]
pub enum KeyringUpdate {
    /// Swap encrypted metadata with new name.
    #[serde(rename = "rename")]
    Rename {
        keyring: String,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    },
    /// Swap encrypted metadata with new description.
    #[serde(rename = "updateDescription")]
    UpdateDescription {
        keyring: String,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    },
    /// Add a new member (carries DID + public key + role for key wrapping).
    #[serde(rename = "addMember")]
    AddMember {
        keyring: String,
        member_did: String,
        member_public_key: AtBytes,
        role: String,
        created_at: String,
    },
    /// Remove a member (triggers key rotation on the daemon side).
    #[serde(rename = "removeMember")]
    RemoveMember {
        keyring: String,
        member_did: String,
        created_at: String,
    },
    /// Change a member's role.
    #[serde(rename = "updateRole")]
    UpdateRole {
        keyring: String,
        member_did: String,
        role: String,
        created_at: String,
    },
    /// Leave the workspace. AppView handles visibility immediately;
    /// the owner's daemon processes key rotation asynchronously.
    #[serde(rename = "leave")]
    Leave { keyring: String, created_at: String },
}

impl KeyringUpdateRecord {
    fn new(update: KeyringUpdate) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            update,
        }
    }

    pub fn rename(
        keyring: String,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self::new(KeyringUpdate::Rename {
            keyring,
            encrypted_metadata,
            created_at,
        })
    }

    pub fn update_description(
        keyring: String,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self::new(KeyringUpdate::UpdateDescription {
            keyring,
            encrypted_metadata,
            created_at,
        })
    }

    pub fn add_member(
        keyring: String,
        member_did: String,
        member_public_key: Vec<u8>,
        role: String,
        created_at: String,
    ) -> Self {
        Self::new(KeyringUpdate::AddMember {
            keyring,
            member_did,
            member_public_key: AtBytes::from_raw(&member_public_key),
            role,
            created_at,
        })
    }

    pub fn remove_member(keyring: String, member_did: String, created_at: String) -> Self {
        Self::new(KeyringUpdate::RemoveMember {
            keyring,
            member_did,
            created_at,
        })
    }

    pub fn update_role(
        keyring: String,
        member_did: String,
        role: String,
        created_at: String,
    ) -> Self {
        Self::new(KeyringUpdate::UpdateRole {
            keyring,
            member_did,
            role,
            created_at,
        })
    }

    pub fn leave(keyring: String, created_at: String) -> Self {
        Self::new(KeyringUpdate::Leave {
            keyring,
            created_at,
        })
    }
}

impl KeyringUpdate {
    /// The keyring URI this update targets.
    pub fn keyring(&self) -> &str {
        match self {
            Self::Rename { keyring, .. }
            | Self::UpdateDescription { keyring, .. }
            | Self::AddMember { keyring, .. }
            | Self::RemoveMember { keyring, .. }
            | Self::UpdateRole { keyring, .. }
            | Self::Leave { keyring, .. } => keyring,
        }
    }
}
