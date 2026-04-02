// Daemon scheduler — background task orchestration in the Comlink Web Worker.
//
// Runs behind a Web Locks leader gate so only one tab executes daemon tasks.
// Tasks are scheduled from the core registry (single source of truth) and
// persisted to IndexedDB via the storage instance from context.ts.
//
// The service worker is NOT used for daemon tasks — SWs have lifecycle limits,
// WASM init issues, and no leader election. This module replaces all of that.

import {
  cleanupExpiredPairRequests,
  defaultPairRequestTtlSeconds,
  healStaleGrants,
  daemonTaskDefs,
} from "@/wasm/opake-wasm/opake";
import { storage, withOpake } from "./context";
import { isAuthError } from "@/lib/authErrors";
import type { Session } from "@/lib/storageTypes";

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/* eslint-disable functional/no-let -- daemon lifecycle state requires mutation */
let intervalIds: readonly ReturnType<typeof setInterval>[] = [];
let releaseLock: (() => void) | null = null;
/* eslint-enable functional/no-let */

const INITIAL_DELAY_MS = 5_000;
const PRUNE_AGE_MS = 7 * 24 * 60 * 60 * 1000; // 7 days

const daemonChannel = new BroadcastChannel("opake-daemon");

// ---------------------------------------------------------------------------
// Leader election + scheduling
// ---------------------------------------------------------------------------

export function startDaemon(): void {
  if (releaseLock) return; // already running

  console.info("[daemon] startDaemon called, locks available:", "locks" in navigator);

  if ("locks" in navigator) {
    void navigator.locks.request("opake-daemon-leader", async () => {
      console.info("[daemon] acquired leader lock");
      await pruneOldTasks();
      scheduleTasks();
      // Hold the lock until stopDaemon() or tab closes
      await new Promise<void>((resolve) => {
        releaseLock = resolve;
      });
      console.debug("[daemon] released leader lock");
    });
  } else {
    // Fallback: no Web Locks support — run unconditionally
    console.info("[daemon] no Web Locks API, running without leader election");
    void pruneOldTasks();
    scheduleTasks();
  }
}

export function stopDaemon(): void {
  intervalIds.forEach((id) => clearInterval(id));
  intervalIds = [];
  if (releaseLock) {
    releaseLock();
    releaseLock = null;
  }
}

function scheduleTasks(): void {
  const tasks = daemonTaskDefs() as readonly { name: string; interval_seconds: number }[];

  intervalIds = tasks.flatMap((task) => {
    const handler = taskHandler(task.name);
    if (!handler) return [];

    const intervalMs = task.interval_seconds * 1000;
    setTimeout(() => void handler(), INITIAL_DELAY_MS);
    const id = setInterval(() => void handler(), intervalMs);

    console.debug(`[daemon] scheduled "${task.name}" every ${task.interval_seconds}s`);
    return [id];
  });
}

// ---------------------------------------------------------------------------
// Task handler registry
// ---------------------------------------------------------------------------

function taskHandler(name: string): (() => Promise<void>) | null {
  switch (name) {
    case "pair-cleanup":
      return () =>
        runTracked("pair-cleanup", { type: "pairCleanup", deleted: 0 }, async () => {
          const ctx = await loadAccountContext();
          if (!ctx) return { didWork: false, finalKind: { type: "pairCleanup", deleted: 0 } };

          // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment -- WASM returns { result, session }
          const response: Readonly<{
            result?: { requests_deleted?: number; responses_deleted?: number };
            session?: Session;
          }> = await cleanupExpiredPairRequests(
            ctx.session,
            ctx.pdsUrl,
            defaultPairRequestTtlSeconds(),
          );
          if (response.session) await storage.saveSession(ctx.did, response.session);

          const deleted =
            (response.result?.requests_deleted ?? 0) + (response.result?.responses_deleted ?? 0);
          return { didWork: deleted > 0, finalKind: { type: "pairCleanup", deleted } };
        });

    case "grant-healing":
      return () =>
        runTracked("grant-healing", { type: "grantHealing", healed: 0 }, async () => {
          const ctx = await loadAccountContext();
          if (!ctx) return { didWork: false, finalKind: { type: "grantHealing", healed: 0 } };

          // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment -- WASM returns { result, session }
          const response: Readonly<{
            result?: { grants_deleted?: number };
            session?: Session;
          }> = await healStaleGrants(ctx.session, ctx.pdsUrl);
          if (response.session) await storage.saveSession(ctx.did, response.session);

          const healed = response.result?.grants_deleted ?? 0;
          return { didWork: healed > 0, finalKind: { type: "grantHealing", healed } };
        });

    case "share-retry":
      return () =>
        runTracked("share-retry", { type: "shareRetry", retried: 0 }, async () => {
          const ctx = await loadAccountContext();
          if (!ctx) return { didWork: false, finalKind: { type: "shareRetry", retried: 0 } };

          const result = (await withOpake((opake) =>
            (
              opake as unknown as { retryPendingSharesViaOpake: () => Promise<unknown> }
            ).retryPendingSharesViaOpake(),
          )) as Readonly<{ completed?: number }>;
          const retried = result.completed ?? 0;
          return { didWork: retried > 0, finalKind: { type: "shareRetry", retried } };
        });

    case "directory-sync":
      return () =>
        runTracked(
          "directory-sync",
          { type: "proposalSync", keyringUri: "", proposalsApplied: 0 },
          async () => {
            const actx = await loadAccountContext();
            if (!actx)
              return {
                didWork: false,
                finalKind: { type: "proposalSync", keyringUri: "", proposalsApplied: 0 },
              };

            const results = (await withOpake((ctx) =>
              (
                ctx as unknown as {
                  syncOwnedWorkspacesDetailed: () => Promise<unknown>;
                }
              ).syncOwnedWorkspacesDetailed(),
            )) as readonly {
              keyring_uri: string;
              proposals_applied: number;
              error?: string;
            }[];

            const totalApplied = results.reduce((sum, r) => sum + r.proposals_applied, 0);
            const firstError = results.find((r) => r.error)?.error;

            // Per-workspace task entries for workspaces that did work
            await Promise.all(
              results
                .filter((r) => r.proposals_applied > 0 || r.error)
                .map((r) =>
                  persistTask(
                    `sync-${r.keyring_uri}`,
                    {
                      type: "proposalSync",
                      keyringUri: r.keyring_uri,
                      proposalsApplied: r.proposals_applied,
                    },
                    r.error ? { failed: r.error } : "completed",
                  ),
                ),
            );

            // Notify the main thread so the workspace store can reload
            if (totalApplied > 0) {
              const updatedUris = results
                .filter((r) => r.proposals_applied > 0)
                .map((r) => r.keyring_uri);
              daemonChannel.postMessage({ type: "workspace-updated", keyringUris: updatedUris });
            }

            return {
              didWork: totalApplied > 0 || firstError != null,
              finalKind: {
                type: "proposalSync",
                keyringUri: "",
                proposalsApplied: totalApplied,
              },
            };
          },
        );

    default:
      return null;
  }
}

// ---------------------------------------------------------------------------
// Task lifecycle: runTracked wrapper
// ---------------------------------------------------------------------------

interface TaskResult {
  readonly didWork: boolean;
  readonly finalKind: Record<string, unknown>;
}

async function runTracked(
  name: string,
  initialKind: Record<string, unknown>,
  fn: () => Promise<TaskResult>,
): Promise<void> {
  const id = `${name}-${new Date().toISOString()}`;
  await persistTask(id, initialKind, "running");

  try {
    const result = await fn();
    if (result.didWork) {
      await persistTask(id, result.finalKind, "completed");
    } else {
      await storage.deleteTask(id);
    }
  } catch (err) {
    console.warn(`[daemon] ${name} failed:`, err);

    if (isAuthError(err)) {
      // Don't expire the session from the daemon — the token may have been
      // rotated by a concurrent main-thread refresh. Silently skip this cycle;
      // the next cycle will pick up the fresh token from IndexedDB.
      await storage.deleteTask(id);
      return;
    }

    await persistTask(id, initialKind, { failed: String(err) });
  }
}

// ---------------------------------------------------------------------------
// Task persistence helpers
// ---------------------------------------------------------------------------

type TaskStatus = "running" | "completed" | { readonly failed: string };

async function persistTask(
  id: string,
  kind: Record<string, unknown>,
  status: TaskStatus,
): Promise<void> {
  const now = new Date().toISOString();
  await storage.saveTask({
    id,
    kind,
    status,
    progress: null,
    createdAt: now,
    updatedAt: now,
  });
}

async function pruneOldTasks(): Promise<void> {
  const all = (await storage.loadTasks()) as readonly { id: string; updatedAt: string }[];
  const cutoff = new Date(Date.now() - PRUNE_AGE_MS).toISOString();
  const stale = all.filter((t) => t.updatedAt < cutoff);
  await Promise.all(stale.map((t) => storage.deleteTask(t.id)));
  if (stale.length > 0) {
    console.debug(`[daemon] pruned ${stale.length} old tasks`);
  }
}

// ---------------------------------------------------------------------------
// Account context loader (mirrors the old SW pattern)
// ---------------------------------------------------------------------------

interface AccountContext {
  readonly did: string;
  readonly pdsUrl: string;
  readonly session: Session;
}

async function loadAccountContext(): Promise<AccountContext | null> {
  const config = await storage.loadConfig();
  if (!config.default_did) return null;

  const did = config.default_did;
  const account = config.accounts[did];
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- Record index may be missing at runtime
  if (!account) return null;

  const session = await storage.loadSession(did);
  return { did, pdsUrl: account.pds_url, session };
}
