// Daemon types — task definitions, results, and configuration.

/** A daemon task definition from the core task registry. */
export interface TaskDef {
  readonly name: string;
  readonly intervalSeconds: number;
  readonly description: string;
}

/** Status of a persisted task record. */
export type TaskStatus = "running" | "completed" | { readonly failed: string };

/** A persisted task entry for UI visibility. */
export interface TaskRecord {
  readonly id: string;
  readonly kind: Readonly<Record<string, unknown>>;
  readonly status: TaskStatus;
  readonly progress: null;
  readonly createdAt: string;
  readonly updatedAt: string;
}

/**
 * SSE-specific daemon configuration. Presence of this field switches the
 * daemon into event-driven mode: `directory-sync` becomes a low-frequency
 * fallback timer while SSE events drive targeted per-workspace sync.
 * Absence keeps the daemon in classic timer-polling mode.
 */
export interface SSEConfig {
  /** Appview base URL for the SSE stream + token exchange. */
  readonly appviewUrl: string;

  /**
   * Called when any SSE event arrives that affects directory/document
   * state (excluding self-events). Unlike `onWorkspaceUpdated`, this fires
   * for every non-self record mutation so the host app can reload the
   * current view.
   */
  readonly onRecordChanged?: () => void;
}

/** Configuration for the daemon scheduler. */
export interface DaemonOptions {
  /**
   * Delay before the first task execution (ms). Gives the app time to
   * finish booting before background work starts.
   * @default 5000
   */
  readonly initialDelayMs?: number;

  /**
   * Age after which completed/failed task records are pruned (ms).
   * @default 604800000 (7 days)
   */
  readonly pruneAgeMs?: number;

  /**
   * SSE streaming config. When present, the daemon subscribes to the
   * appview's event stream and reacts to proposal events with targeted
   * per-workspace sync. When absent, all sync happens via interval polling.
   */
  readonly sse?: SSEConfig;

  /**
   * Called when a workspace's directory tree is updated by proposal
   * application. The daemon can't update UI state directly — this
   * callback lets the host app trigger a reload.
   */
  readonly onWorkspaceUpdated?: (keyringUris: readonly string[]) => void;

  /** Called when the daemon detects an expired session. */
  readonly onSessionExpired?: () => void;
}

/** Task persistence interface — subset of Storage for daemon use. */
export interface TaskStore {
  saveTask(task: TaskRecord): Promise<void>;
  loadTasks(): Promise<readonly TaskRecord[]>;
  deleteTask(id: string): Promise<void>;
}
