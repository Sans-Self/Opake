"use client";

import { useEffect } from "react";
import type { DaemonOptions, TaskDef, TaskStore } from "@opake/daemon";
import { startDaemon } from "@opake/daemon";
import { useOpake } from "../provider";

interface UseDaemonOptions extends DaemonOptions {
  readonly taskDefs: readonly TaskDef[];
  readonly taskStore: TaskStore;
}

/**
 * Start the background daemon (pair-cleanup, grant-healing, share-retry).
 *
 * The daemon is pure maintenance polling in the React package — live
 * tree updates come from the SSE consumer via `useDirectory`, not
 * from daemon task callbacks. If you need react-query cache
 * invalidation on live updates, migrate to `useDirectory` which
 * subscribes to `FileManager.watchDirectory` and receives fresh
 * snapshots as they arrive from SSE.
 *
 * @example
 * ```tsx
 * import { Opake } from "@opake/sdk";
 * useDaemon({
 *   taskDefs: await Opake.taskDefs(),
 *   taskStore,
 *   onSessionExpired: () => logout(),
 * });
 * ```
 */
export function useDaemon(options: UseDaemonOptions): void {
  const opake = useOpake();

  useEffect(() => {
    const handle = startDaemon(opake, options.taskDefs, options.taskStore, {
      initialDelayMs: options.initialDelayMs,
      pruneAgeMs: options.pruneAgeMs,
      onSessionExpired: options.onSessionExpired,
    });

    return () => handle.stop();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- callbacks are stable via closure
  }, [opake, options.taskDefs, options.taskStore]);
}
