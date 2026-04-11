// Daemon scheduler — Web Locks leader election + setInterval orchestration.
//
// When `options.sse` is provided, SSE-driven proposal sync replaces the
// `directory-sync` timer. Other tasks (pair-cleanup, grant-healing,
// share-retry) always run on intervals.
//
// Returns a DaemonHandle that the caller uses to stop the daemon.
// No module-level state — multiple handles can coexist (though only one
// leader runs per Web Locks scope).

import type { Opake } from "@opake/sdk";
import type { DaemonOptions, SSEConfig, TaskDef, TaskStore } from "./types";
import { runTasks } from "./tasks";
import { startSSEConsumer, type SSEConsumerHandle } from "./sse-consumer";

const DEFAULT_INITIAL_DELAY_MS = 5_000;
const DEFAULT_PRUNE_AGE_MS = 7 * 24 * 60 * 60 * 1000; // 7 days
// Reduced interval for directory-sync when SSE is active. SSE handles most
// sync, but document_update proposals can't route to workspace topics (the
// lexicon has no keyring field) so this timer catches what SSE misses.
const DIRECTORY_SYNC_FALLBACK_MS = 60_000;

/** Handle returned by `startDaemon` — call `stop()` to shut down. */
export interface DaemonHandle {
  /** Stop all intervals, SSE subscription, and release the leader lock. */
  stop(): void;
}

/**
 * Start the background daemon.
 *
 * Acquires a Web Locks leader lock (if available) so only one tab runs
 * background tasks. Schedules all tasks from the core registry at their
 * configured intervals.
 *
 * When `options.sse` is provided, SSE-driven proposal sync replaces the
 * normal `directory-sync` interval — proposal events trigger immediate
 * targeted syncs, and the `directory-sync` timer downgrades to a
 * low-frequency fallback for events SSE can't route.
 *
 * @returns A handle to stop the daemon.
 *
 * @example
 * ```typescript
 * import { Opake } from "@opake/sdk";
 * import { startDaemon } from "@opake/daemon";
 *
 * const opake = await Opake.init();
 * const daemon = startDaemon(opake, taskDefs, taskStore, {
 *   sse: {
 *     appviewUrl: "https://appview.opake.app",
 *     onRecordChanged: () => reloadCurrentView(),
 *   },
 *   onWorkspaceUpdated: (uris) => reloadWorkspace(uris),
 * });
 *
 * // Later:
 * daemon.stop();
 * ```
 */
export function startDaemon(
  opake: Opake,
  taskDefs: readonly TaskDef[],
  taskStore: TaskStore,
  options?: DaemonOptions,
): DaemonHandle {
  const intervalIds: ReturnType<typeof setInterval>[] = [];
  // eslint-disable-next-line functional/no-let -- mutable ref for cleanup
  let releaseLock: (() => void) | null = null;
  // eslint-disable-next-line functional/no-let -- mutable ref for cleanup
  let sseConsumer: SSEConsumerHandle | null = null;

  const sse: SSEConfig | undefined = options?.sse;

  function stop(): void {
    sseConsumer?.close();
    sseConsumer = null;
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

      // When SSE is active, directory-sync becomes a low-frequency fallback
      // (60s) instead of the normal interval. SSE handles most proposal sync,
      // but document_update events can't be workspace-routed (no keyring_uri
      // in the lexicon) so the timer catches what SSE misses.
      const intervalMs = (sse && task.name === "directory-sync")
        ? DIRECTORY_SYNC_FALLBACK_MS
        : task.intervalSeconds * 1000;

      setTimeout(() => void handler(), initialDelay);
      intervalIds.push(setInterval(() => void handler(), intervalMs));
    }

    if (sse) {
      sseConsumer = startSSEConsumer(opake, taskStore, options ?? {}, sse);
    }
  }

  const hasWebLocks = typeof navigator !== "undefined" && "locks" in navigator;

  if (hasWebLocks) {
    void navigator.locks.request("opake-daemon-leader", async () => {
      await pruneOldTasks(
        taskStore,
        options?.pruneAgeMs ?? DEFAULT_PRUNE_AGE_MS,
      );
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

async function pruneOldTasks(
  taskStore: TaskStore,
  pruneAgeMs: number,
): Promise<void> {
  const all = await taskStore.loadTasks();
  const cutoff = new Date(Date.now() - pruneAgeMs).toISOString();
  const stale = all.filter((t) => t.updatedAt < cutoff);
  await Promise.all(stale.map((t) => taskStore.deleteTask(t.id)));
}
