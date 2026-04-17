// Task handler implementations.
//
// Each handler calls operations on the Opake instance and reports results
// via the TaskStore. The handlers don't know about scheduling — they're
// called by the scheduler at the configured intervals.
//
// Proposal sync is not represented here as a timer task: web clients
// drive proposal application via SSE events (see the WASM consumer in
// `crates/opake-wasm/src/sse_wasm.rs`), and the native CLI daemon runs
// its own native SSE consumer. There's no fallback polling in either
// track — the proposal store in the indexer DB is the backstop if a
// real-time event is missed.

import { OpakeError } from "@opake/sdk";
import type { Opake } from "@opake/sdk";
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
      tracked(
        taskStore,
        "pair-cleanup",
        { type: "pairCleanup", deleted: 0 },
        async () => {
          const deleted = await opake.cleanupExpiredPairRequests();
          return { didWork: deleted > 0, kind: { type: "pairCleanup", deleted } };
        },
        options,
      ),

    "grant-healing": () =>
      tracked(
        taskStore,
        "grant-healing",
        { type: "grantHealing", healed: 0 },
        async () => {
          const healed = await opake.healStaleGrants();
          return { didWork: healed > 0, kind: { type: "grantHealing", healed } };
        },
        options,
      ),

    "share-retry": () =>
      tracked(
        taskStore,
        "share-retry",
        { type: "shareRetry", retried: 0 },
        async () => {
          const result = await opake.retryPendingShares();
          const retried = result.completed;
          return { didWork: retried > 0, kind: { type: "shareRetry", retried } };
        },
        options,
      ),
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
function handleDaemonError(err: unknown, options?: DaemonOptions): boolean {
  if (err instanceof OpakeError && err.kind === "Auth") {
    options?.onSessionExpired?.();
    return true;
  }
  return false;
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
