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
        name: "member-wrap-repair",
        interval_seconds: 10 * 60,
        description: "Repair admitted members' missing current group-key wraps when authorized",
    },
    // Note: proposal sync is no longer a timer-polling task. The web
    // client runs a WASM-owned SSE consumer, the CLI daemon runs a
    // native `SseConsumer` (via `ReqwestSseTransport`), and both route
    // proposal events directly to `Opake::sync_workspace_by_uri`. The
    // pre-SSE `directory-sync` TaskDef was removed when the CLI SSE
    // migration landed.
];

/// Look up a task by name.
pub fn task_by_name(name: &str) -> Option<&'static TaskDef> {
    TASKS.iter().find(|t| t.name == name)
}

/// The default session refresh threshold, re-exported for convenience.
pub const SESSION_REFRESH_THRESHOLD: i64 = DEFAULT_REFRESH_THRESHOLD_SECONDS;

// ---------------------------------------------------------------------------
// Background task tracking
// ---------------------------------------------------------------------------

use serde::{Deserialize, Serialize};

/// Result of syncing a single workspace (chain head load + optional error).
///
/// Never fails at the Result level — per-workspace errors are captured in
/// `error` so the caller can continue with remaining workspaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSyncResult {
    pub keyring_uri: String,
    pub is_owner: bool,
    pub error: Option<String>,
}

/// A tracked background task (persisted via Storage trait).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonTask {
    pub id: String,
    pub kind: DaemonTaskKind,
    pub status: TaskStatus,
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
    /// Repaired missing current member wraps from the live keyring heads.
    /// Pending approvals and verification failures remain derivable in those
    /// heads and are never treated as completed work.
    /// spec:background-work § Remaining work is derived from records, never stored
    MemberWrapRepair { repaired: usize },
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
