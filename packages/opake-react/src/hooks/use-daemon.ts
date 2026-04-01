import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import type { DaemonOptions, TaskDef, TaskStore } from "@opake/daemon";
import { startDaemon } from "@opake/daemon";
import { useOpake } from "../provider";
import { opakeKeys } from "../keys";

interface UseDaemonOptions extends Omit<DaemonOptions, "onWorkspaceUpdated"> {
  readonly taskDefs: readonly TaskDef[];
  readonly taskStore: TaskStore;
  readonly onWorkspaceUpdated?: (keyringUris: readonly string[]) => void;
}

/**
 * Start the background daemon and integrate with React Query.
 *
 * Automatically invalidates workspace tree queries when the daemon
 * applies proposals. Stops the daemon on unmount.
 *
 * @example
 * ```tsx
 * useDaemon({ taskDefs, taskStore });
 * ```
 */
export function useDaemon(options: UseDaemonOptions): void {
  const opake = useOpake();
  const queryClient = useQueryClient();

  // Ref for callbacks — avoids restarting daemon when callbacks change
  const callbacksRef = useRef(options);
  callbacksRef.current = options;

  useEffect(() => {
    const handle = startDaemon(opake, options.taskDefs, options.taskStore, {
      initialDelayMs: options.initialDelayMs,
      pruneAgeMs: options.pruneAgeMs,
      onSessionExpired: () => callbacksRef.current.onSessionExpired?.(),
      onWorkspaceUpdated: (uris) => {
        for (const uri of uris) {
          void queryClient.invalidateQueries({ queryKey: opakeKeys.workspaceTree(uri) });
        }
        void queryClient.invalidateQueries({ queryKey: opakeKeys.workspaces() });
        callbacksRef.current.onWorkspaceUpdated?.(uris);
      },
    });

    return () => handle.stop();
  }, [opake, options.taskDefs, options.taskStore, queryClient]);
}
