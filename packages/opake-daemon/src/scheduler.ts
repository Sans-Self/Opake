// Daemon scheduler — Web Locks leader election + setInterval orchestration.
//
// Runs whichever tasks have handlers in `runTasks` (pair-cleanup,
// grant-healing, share-retry). Task definitions without a matching
// handler are silently skipped. Proposal sync is not a timer task —
// it flows through the WASM SSE consumer instead; see `tasks.ts` for
// the full explanation.
//
// Returns a DaemonHandle that the caller uses to stop the daemon.
// No module-level state — multiple handles can coexist (though only one
// leader runs per Web Locks scope).

import type { Opake } from "@opake/sdk";
import type { DaemonOptions, TaskDef, TaskStore } from "./types";
import { runTasks } from "./tasks";

const DEFAULT_INITIAL_DELAY_MS = 5_000;
const DEFAULT_PRUNE_AGE_MS = 7 * 24 * 60 * 60 * 1000; // 7 days

/** Handle returned by `startDaemon` — call `stop()` to shut down. */
export interface DaemonHandle {
  /** Stop all intervals and release the leader lock. */
  stop(): void;
}

/**
 * Start the background daemon.
 *
 * Acquires a Web Locks leader lock (if available) so only one tab runs
 * background tasks. Schedules all tasks from the core registry at their
 * configured intervals.
 *
 * For live tree updates, pair this with `opake.startSseConsumer(indexerUrl)`
 * and `fileManager.watchDirectory(uri, handler)` — the WASM-side consumer
 * handles SSE events directly and patches trees in place.
 *
 * @returns A handle to stop the daemon.
 *
 * @example
 * ```typescript
 * import { Opake } from "@opake/sdk";
 * import { startDaemon } from "@opake/daemon";
 *
 * const opake = await Opake.init();
 * await opake.startSseConsumer();
 * const daemon = startDaemon(opake, taskDefs, taskStore, {
 *   onSessionExpired: () => redirectToLogin(),
 * });
 *
 * // Later (shutdown):
 * daemon.stop();
 * opake.stopSseConsumer();
 * opake.wipeState(); // drops decrypted keepers — only on logout/session switch
 * ```
 */
export function startDaemon(
  opake: Opake,
  taskDefs: readonly TaskDef[],
  taskStore: TaskStore,
  options?: DaemonOptions,
): DaemonHandle {
  const intervalIds: ReturnType<typeof setInterval>[] = [];
  let releaseLock: (() => void) | null = null;

  function stop(): void {
    for (const id of intervalIds) {
      clearInterval(id);
    }
    intervalIds.length = 0;
    if (releaseLock) {
      releaseLock();
      releaseLock = null;
    }
  }

  function scheduleAll(): void {
    const initialDelay = options?.initialDelayMs ?? DEFAULT_INITIAL_DELAY_MS;
    const handlers = runTasks(opake, taskStore, options);

    for (const task of taskDefs) {
      const handler = handlers[task.name];
      if (!handler) continue;

      const intervalMs = task.intervalSeconds * 1000;
      setTimeout(() => void handler(), initialDelay);
      intervalIds.push(setInterval(() => void handler(), intervalMs));
    }
  }

  const hasWebLocks = typeof navigator !== "undefined" && "locks" in navigator;

  if (hasWebLocks) {
    void navigator.locks.request("opake-daemon-leader", async () => {
      await pruneOldTasks(taskStore, options?.pruneAgeMs ?? DEFAULT_PRUNE_AGE_MS);
      scheduleAll();
      await new Promise<void>((resolve) => {
        releaseLock = resolve;
      });
    });
  } else {
    void pruneOldTasks(taskStore, options?.pruneAgeMs ?? DEFAULT_PRUNE_AGE_MS);
    scheduleAll();
  }

  return { stop };
}

async function pruneOldTasks(taskStore: TaskStore, pruneAgeMs: number): Promise<void> {
  const all = await taskStore.loadTasks();
  const cutoff = new Date(Date.now() - pruneAgeMs).toISOString();
  const stale = all.filter((t) => t.updatedAt < cutoff);
  await Promise.all(stale.map((t) => taskStore.deleteTask(t.id)));
}
