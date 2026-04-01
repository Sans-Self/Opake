use serde::{Deserialize, Serialize};

use super::{default_version, EncryptedMetadata, SCHEMA_VERSION};

pub const DIRECTORY_UPDATE_COLLECTION: &str = "app.opake.directoryUpdate";

/// Action type strings for matching AppView proposal responses.
pub const ACTION_ADD_ENTRY: &str = "addEntry";
pub const ACTION_REMOVE_ENTRY: &str = "removeEntry";
pub const ACTION_MOVE_ENTRY: &str = "moveEntry";
pub const ACTION_CREATE_DIRECTORY: &str = "createDirectory";
pub const ACTION_DELETE_DIRECTORY: &str = "deleteDirectory";
pub const ACTION_RENAME_DIRECTORY: &str = "renameDirectory";

/// A proposed structural change to a workspace directory, with schema version envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryUpdateRecord {
    #[serde(default = "default_version")]
    pub opake_version: u32,
    #[serde(flatten)]
    pub update: DirectoryUpdate,
}

/// The actual directory update, discriminated by `actionType`.
///
/// Field names use camelCase on the wire (PDS records are AT Protocol JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "actionType", rename_all = "camelCase")]
pub enum DirectoryUpdate {
    /// Place a document or directory into a target directory.
    #[serde(rename = "addEntry", rename_all = "camelCase")]
    AddEntry {
        keyring: String,
        directory: String,
        entry: String,
        created_at: String,
    },
    /// Remove an entry from a directory.
    #[serde(rename = "removeEntry", rename_all = "camelCase")]
    RemoveEntry {
        keyring: String,
        directory: String,
        entry: String,
        created_at: String,
    },
    /// Atomic move from source to target directory.
    #[serde(rename = "moveEntry", rename_all = "camelCase")]
    MoveEntry {
        keyring: String,
        source_directory: String,
        target_directory: String,
        entry: String,
        created_at: String,
    },
    /// Create a new subdirectory under a parent.
    #[serde(rename = "createDirectory", rename_all = "camelCase")]
    CreateDirectory {
        keyring: String,
        parent_directory: String,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    },
    /// Delete an empty directory.
    #[serde(rename = "deleteDirectory", rename_all = "camelCase")]
    DeleteDirectory {
        keyring: String,
        directory: String,
        created_at: String,
    },
    /// Rename a directory (re-encrypted metadata).
    #[serde(rename = "renameDirectory", rename_all = "camelCase")]
    RenameDirectory {
        keyring: String,
        directory: String,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    },
}

impl DirectoryUpdateRecord {
    fn new(update: DirectoryUpdate) -> Self {
        Self {
            opake_version: SCHEMA_VERSION,
            update,
        }
    }

    pub fn add_entry(
        keyring: String,
        directory: String,
        entry: String,
        created_at: String,
    ) -> Self {
        Self::new(DirectoryUpdate::AddEntry {
            keyring,
            directory,
            entry,
            created_at,
        })
    }

    pub fn remove_entry(
        keyring: String,
        directory: String,
        entry: String,
        created_at: String,
    ) -> Self {
        Self::new(DirectoryUpdate::RemoveEntry {
            keyring,
            directory,
            entry,
            created_at,
        })
    }

    pub fn move_entry(
        keyring: String,
        source_directory: String,
        target_directory: String,
        entry: String,
        created_at: String,
    ) -> Self {
        Self::new(DirectoryUpdate::MoveEntry {
            keyring,
            source_directory,
            target_directory,
            entry,
            created_at,
        })
    }

    pub fn create_directory(
        keyring: String,
        parent_directory: String,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self::new(DirectoryUpdate::CreateDirectory {
            keyring,
            parent_directory,
            encrypted_metadata,
            created_at,
        })
    }

    pub fn delete_directory(keyring: String, directory: String, created_at: String) -> Self {
        Self::new(DirectoryUpdate::DeleteDirectory {
            keyring,
            directory,
            created_at,
        })
    }

    pub fn rename_directory(
        keyring: String,
        directory: String,
        encrypted_metadata: EncryptedMetadata,
        created_at: String,
    ) -> Self {
        Self::new(DirectoryUpdate::RenameDirectory {
            keyring,
            directory,
            encrypted_metadata,
            created_at,
        })
    }
}

impl DirectoryUpdate {
    /// The keyring URI this update targets.
    pub fn keyring(&self) -> &str {
        match self {
            Self::AddEntry { keyring, .. }
            | Self::RemoveEntry { keyring, .. }
            | Self::MoveEntry { keyring, .. }
            | Self::CreateDirectory { keyring, .. }
            | Self::DeleteDirectory { keyring, .. }
            | Self::RenameDirectory { keyring, .. } => keyring,
        }
    }
}
