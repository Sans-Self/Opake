// Daemon task registry.
//
// Defines what background maintenance tasks exist and at what cadence they
// run. The actual execution is platform-specific (CLI uses tokio + FileStorage,
// web uses Service Worker + IndexedDB), but the task definitions are shared
// so both platforms stay in sync.

use crate::client::session_refresh::DEFAULT_REFRESH_THRESHOLD_SECONDS;
use crate::pairing::DEFAULT_PAIR_REQUEST_TTL_SECONDS;

/// A background maintenance task the daemon should run.
#[derive(Debug, Clone)]
pub struct TaskDef {
    /// Identifier used for logging and message routing.
    pub name: &'static str,
    /// How often to run this task, in seconds.
    pub interval_seconds: i64,
    /// Human-readable description for help text.
    pub description: &'static str,
}

/// All daemon tasks in the order they should be registered.
pub const TASKS: &[TaskDef] = &[
    TaskDef {
        name: "session-refresh",
        interval_seconds: 60,
        description: "Refresh OAuth access tokens before they expire",
    },
    TaskDef {
        name: "pair-cleanup",
        // Run at half the TTL so expired requests are caught before they've
        // been stale for a full TTL window.
        interval_seconds: DEFAULT_PAIR_REQUEST_TTL_SECONDS / 2,
        description: "Delete expired pair requests and orphaned responses",
    },
    TaskDef {
        name: "grant-healing",
        interval_seconds: 20 * 60,
        description: "Delete grants whose recipient has no valid public key",
    },
    TaskDef {
        name: "share-retry",
        interval_seconds: 300,
        description: "Retry pending shares for recipients who haven't set up yet",
    },
    TaskDef {
        name: "directory-sync",
        interval_seconds: 5,
        description: "Apply pending directory updates from workspace members",
    },
];

/// Look up a task by name.
pub fn task_by_name(name: &str) -> Option<&'static TaskDef> {
    TASKS.iter().find(|t| t.name == name)
}

/// The default session refresh threshold, re-exported for convenience.
pub const SESSION_REFRESH_THRESHOLD: i64 = DEFAULT_REFRESH_THRESHOLD_SECONDS;

// ---------------------------------------------------------------------------
// Background task tracking (re-encryption, etc.)
// ---------------------------------------------------------------------------

use serde::{Deserialize, Serialize};

/// Debounce window for re-encryption after member removal (seconds).
pub const REENCRYPTION_DEBOUNCE_SECONDS: i64 = 180;

/// Maximum blob data to process per re-encryption batch.
pub const REENCRYPTION_BATCH_SIZE_BYTES: u64 = 500 * 1024 * 1024;

/// Result of syncing a single workspace (proposals applied + optional error).
///
/// Never fails at the Result level — per-workspace errors are captured in
/// `error` so the caller can continue with remaining workspaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSyncResult {
    pub keyring_uri: String,
    pub is_owner: bool,
    pub proposals_applied: usize,
    pub proposals_cleaned_up: usize,
    pub error: Option<String>,
}

/// A tracked background task (persisted via Storage trait).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonTask {
    pub id: String,
    pub kind: DaemonTaskKind,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<TaskProgress>,
    pub created_at: String,
    pub updated_at: String,
}

/// What the task does. Each variant maps to a daemon task from the TASKS
/// registry. Only persisted when the task did meaningful work or failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DaemonTaskKind {
    /// Proactively refreshed an OAuth session before expiry.
    SessionRefresh,
    /// Deleted expired pair requests and orphaned responses.
    PairCleanup { deleted: usize },
    /// Deleted grants whose recipient has no valid public key.
    GrantHealing { healed: usize },
    /// Retried pending shares for recipients who hadn't set up yet.
    ShareRetry { retried: usize },
    /// Applied directory/keyring/document proposals for a workspace.
    ProposalSync {
        keyring_uri: String,
        proposals_applied: usize,
    },
    /// Re-wrap content keys from an old group key rotation to the current one.
    ReEncryption {
        keyring_uri: String,
        from_rotation: u64,
        to_rotation: u64,
    },
}

/// Current lifecycle state of a daemon task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
}

/// Progress for a running task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgress {
    /// Documents processed by this batch so far.
    pub completed: usize,
    /// Documents still at the old rotation (dynamic — decreases as other
    /// operations migrate documents).
    pub remaining: usize,
    /// Approximate blob bytes processed (for UI display).
    pub bytes_processed: u64,
}
