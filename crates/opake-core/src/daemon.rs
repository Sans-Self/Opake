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
];

/// Look up a task by name.
pub fn task_by_name(name: &str) -> Option<&'static TaskDef> {
    TASKS.iter().find(|t| t.name == name)
}

/// The default session refresh threshold, re-exported for convenience.
pub const SESSION_REFRESH_THRESHOLD: i64 = DEFAULT_REFRESH_THRESHOLD_SECONDS;
