// Task handler implementations.
//
// Each handler calls operations on the Opake instance and reports results
// via the TaskStore. The handlers don't know about scheduling — they're
// called by the scheduler at the configured intervals.

import type { Opake, WorkspaceSyncResult } from "@opake/sdk";
import { OpakeError } from "@opake/sdk";
import type { DaemonOptions, TaskRecord, TaskStore } from "./types";

type Handler = () => Promise<void>;

/** Build the handler map for all known tasks. */
export function runTasks(
  opake: Opake,
  taskStore: TaskStore,
  options?: DaemonOptions,
): Readonly<Record<string, Handler>> {
  return {
    "pair-cleanup": () =>
      tracked(taskStore, "pair-cleanup", { type: "pairCleanup", deleted: 0 }, async () => {
        const deleted = await opake.cleanupExpiredPairRequests();
        return { didWork: deleted > 0, kind: { type: "pairCleanup", deleted } };
      }, options),

    "grant-healing": () =>
      tracked(taskStore, "grant-healing", { type: "grantHealing", healed: 0 }, async () => {
        const healed = await opake.healStaleGrants();
        return { didWork: healed > 0, kind: { type: "grantHealing", healed } };
      }, options),

    "share-retry": () =>
      tracked(taskStore, "share-retry", { type: "shareRetry", retried: 0 }, async () => {
        const result = await opake.retryPendingShares();
        const retried = result.completed ?? 0;
        return { didWork: retried > 0, kind: { type: "shareRetry", retried } };
      }, options),

    "directory-sync": () =>
      tracked(taskStore, "directory-sync", { type: "proposalSync", proposalsApplied: 0 }, async () => {
        const results = await opake.syncOwnedWorkspacesDetailed();

        const totalApplied = results.reduce(
          (sum: number, r: WorkspaceSyncResult) => sum + r.proposalsApplied,
          0,
        );

        await Promise.all(
          results
            .filter((r: WorkspaceSyncResult) => r.proposalsApplied > 0 || r.error)
            .map((r: WorkspaceSyncResult) => persistSyncResult(taskStore, r)),
        );

        if (totalApplied > 0 && options?.onWorkspaceUpdated) {
          const updatedUris = results
            .filter((r: WorkspaceSyncResult) => r.proposalsApplied > 0)
            .map((r: WorkspaceSyncResult) => r.keyringUri);
          options.onWorkspaceUpdated(updatedUris);
        }

        return {
          didWork: totalApplied > 0,
          kind: { type: "proposalSync", proposalsApplied: totalApplied },
        };
      }, options),
  };
}

// ---------------------------------------------------------------------------
// Tracked execution wrapper
// ---------------------------------------------------------------------------

interface TaskResult {
  readonly didWork: boolean;
  readonly kind: Readonly<Record<string, unknown>>;
}

async function tracked(
  taskStore: TaskStore,
  name: string,
  initialKind: Readonly<Record<string, unknown>>,
  fn: () => Promise<TaskResult>,
  options?: DaemonOptions,
): Promise<void> {
  const id = `${name}-${new Date().toISOString()}`;
  const createdAt = new Date().toISOString();

  try {
    const result = await fn();
    if (result.didWork) {
      await persistTask(taskStore, id, result.kind, "completed", createdAt);
    }
    // No work done — don't persist at all (avoids write amplification)
  } catch (err) {
    if (handleDaemonError(err, options)) return;
    await persistTask(taskStore, id, initialKind, { failed: String(err) }, createdAt);
  }
}

/**
 * Shared error branch for daemon operations. Returns true if the error was
 * handled (caller should return early), false if it should be persisted or
 * propagated. Auth errors fire `onSessionExpired` and are considered handled.
 */
export function handleDaemonError(err: unknown, options?: DaemonOptions): boolean {
  if (err instanceof OpakeError && err.kind === "Auth") {
    options?.onSessionExpired?.();
    return true;
  }
  return false;
}

/** Persist a single workspace sync result as a task record. */
export async function persistSyncResult(
  taskStore: TaskStore,
  result: WorkspaceSyncResult,
): Promise<void> {
  await persistTask(
    taskStore,
    `sync-${result.keyringUri}`,
    { type: "proposalSync", keyringUri: result.keyringUri, proposalsApplied: result.proposalsApplied },
    result.error ? { failed: result.error } : "completed",
  );
}

async function persistTask(
  taskStore: TaskStore,
  id: string,
  kind: Readonly<Record<string, unknown>>,
  status: TaskRecord["status"],
  createdAt?: string,
): Promise<void> {
  const now = new Date().toISOString();
  await taskStore.saveTask({
    id,
    kind,
    status,
    progress: null,
    createdAt: createdAt ?? now,
    updatedAt: now,
  });
}
