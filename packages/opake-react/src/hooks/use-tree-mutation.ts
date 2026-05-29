// Shared helper for tree mutation hooks — eliminates repeated
// FileManager lifecycle, optimistic rollback, query invalidation, and
// chain-fork retry.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQueryClient, type UseMutationResult } from "@tanstack/react-query";
import type { ChainForkedEvent, DirectoryTreeSnapshot, FileManager } from "@opake/sdk";
import { useChainForkBus, useFileManagerCache, useOptimisticOverlay } from "../provider";
import type { FileManagerCache } from "../file-manager-cache";
import { scopeKey } from "../optimistic-overlay";
import { opakeKeys } from "../keys";
import { useWorkspaces } from "./use-workspaces";

// Federation: writes commit on the caller's PDS in a single applyWrites
// before the SDK call resolves. The optimistic patch only bridges the
// `onMutate → onSettled` window; we release it on settle and let the
// SSE echo refresh the underlying snapshot. If a concurrent supersede
// wins the race against the chain head, the indexer emits
// `chain:forked` and we replay the mutation with exponential backoff.
const MAX_FORK_RETRIES = 4;
const FORK_RETRY_BASE_MS = 200; // 200ms → 400ms → 800ms → 1600ms (≈3s total)
const FORK_RETRY_MAX_JITTER_MS = 100;

/**
 * Chain-fork retry lifecycle, exposed alongside the standard mutation
 * result. Distinct from the mutation's own `status` because the original
 * mutation has already succeeded by the time retry runs — the chain-fork
 * is a post-success contention signal.
 *
 * - `idle`     — no retry in flight; either no fork happened, or the last
 *                retry succeeded.
 * - `retrying` — at least one fork has hit, and we're cycling through the
 *                backoff schedule. The mutation may complete and return
 *                to `idle` if a replay attempt lands cleanly.
 * - `exhausted` — `MAX_FORK_RETRIES` retries failed. The user has lost
 *                 the race repeatedly; surface a manual-retry affordance.
 */
export type ForkRetryState = "idle" | "retrying" | "exhausted";

/**
 * Return value from `useTreeMutation`. Wraps `UseMutationResult` with
 * the federation-specific fork-retry surface.
 *
 * Type alias rather than `interface extends` because React Query's
 * `UseMutationResult` is a discriminated union (its `status`/`data`/`error`
 * fields form correlated unions), which interfaces can't extend.
 */
export type TreeMutationResult<TResult, TInput> = UseMutationResult<TResult, Error, TInput> & {
  /** Current chain-fork retry lifecycle state. */
  readonly forkRetryState: ForkRetryState;
  /**
   * Reset `forkRetryState` to `"idle"` after surfacing an exhaustion.
   * Call sites that render an exhaustion UI use this to dismiss it,
   * either implicitly (next mutation) or explicitly (user clicks
   * "dismiss" on the retry-failed banner).
   */
  readonly dismissForkRetry: () => void;
};

/**
 * Acquire a FileManager from the provider cache, run a callback,
 * release. Concurrent acquires share a single FileManager instance,
 * so a burst of mutations no longer re-fetches the workspace keyring
 * record from the PDS on every call.
 */
export async function withFileManager<T>(
  cache: FileManagerCache,
  keyringUri: string | null,
  fn: (fm: FileManager) => Promise<T>,
): Promise<T> {
  const fm = await cache.acquire(keyringUri);
  try {
    return await fn(fm);
  } finally {
    cache.release(keyringUri);
  }
}

/** Resolve the React Query cache key for a tree (cabinet or workspace). */
export function treeKeyFor(keyringUri: string | null): readonly unknown[] {
  return keyringUri ? opakeKeys.workspaceTree(keyringUri) : opakeKeys.cabinetTree();
}

interface TreeMutationOptions<TInput, TResult> {
  /** The workspace keyring URI — null for cabinet. */
  readonly keyringUri: string | null;
  /** The actual mutation (receives a FileManager). */
  readonly mutationFn: (fm: FileManager, input: TInput) => Promise<TResult>;
  /**
   * Optimistic tree update — return the updated snapshot.
   * Return the original to skip optimistic update.
   */
  readonly optimisticUpdate?: (
    snapshot: DirectoryTreeSnapshot,
    input: TInput,
  ) => DirectoryTreeSnapshot;
}

interface MutationContext {
  readonly previous?: DirectoryTreeSnapshot;
  readonly releaseOverlay?: () => void;
}

/**
 * Decide whether a chain-fork event affects this mutation's scope.
 *
 * Cabinet mutations are never workspace-forked — the cabinet's directory
 * chain runs entirely on the caller's PDS with no concurrent writers.
 *
 * Workspace mutations are scoped by the workspace's **genesis** URI,
 * which is the value the indexer emits as `workspaceId` on a `chain:forked`
 * event. The SDK's `keyringUri` parameter, by contrast, points at the
 * current keyring chain head and advances on every rotation — those two
 * URIs diverge the moment a workspace is superseded (member add/remove,
 * key rotation). Comparing against the head would silently drop every
 * fork event for any rotated workspace; the caller must supply the
 * stable genesis URI as `workspaceId` so the comparison holds across
 * rotations.
 *
 * `workspaceId` falls back to `keyringUri` only as a transitional
 * compatibility shim — pre-rotation, the two are equal, so the lookup
 * still resolves correctly. Hooks that have access to the full
 * `WorkspaceEntry` should always pass `workspaceId` explicitly.
 */
function forkAffectsScope(
  keyringUri: string | null,
  workspaceId: string | null,
  event: ChainForkedEvent,
): boolean {
  if (keyringUri === null) return false;
  const scope = workspaceId ?? keyringUri;
  return event.workspaceId === scope;
}

/** Exponential-backoff sleep with light jitter. */
function backoffSleep(attempt: number): Promise<void> {
  const base = FORK_RETRY_BASE_MS * 2 ** attempt;
  // eslint-disable-next-line sonarjs/pseudo-random -- jitter for retry timing, not crypto
  const jitter = Math.random() * FORK_RETRY_MAX_JITTER_MS;
  return new Promise((resolve) => setTimeout(resolve, base + jitter));
}

/**
 * Generic tree mutation hook with FileManager lifecycle, optimistic
 * updates, rollback on error, query invalidation on settle, and
 * chain-fork retry with exponential backoff.
 *
 * The return value extends React Query's `UseMutationResult` with
 * `forkRetryState` + `dismissForkRetry` for the chain-fork retry
 * surface. Call sites that want to render an "your write lost a race,
 * still retrying" or "retry exhausted, please try again" affordance
 * read `forkRetryState`; everything else can ignore the extra fields
 * and use the hook exactly like a vanilla `useMutation`.
 */
export function useTreeMutation<TInput, TResult>(
  options: TreeMutationOptions<TInput, TResult>,
): TreeMutationResult<TResult, TInput> {
  const cache = useFileManagerCache();
  const overlay = useOptimisticOverlay();
  const forkBus = useChainForkBus();
  const queryClient = useQueryClient();
  const key = treeKeyFor(options.keyringUri);
  const scope = scopeKey(options.keyringUri);

  // Resolve the workspace's genesis URI for fork-scope matching.
  // The caller passes `keyringUri = workspace.headUri` (advances on
  // every rotation), but `chain:forked` events carry `workspaceId =
  // genesis URI` (stable across the chain's lifetime). Map head → genesis
  // via the live workspace list so the comparison survives rotation.
  //
  // Pre-rotation `workspaceId === headUri`, so the lookup misses on a
  // fresh workspace — that's correct and harmless (fallback compares
  // against keyringUri inside `forkAffectsScope`). Once the workspaces
  // list resolves, subsequent renders pick up the right genesis URI.
  const { data: workspaces } = useWorkspaces();
  const workspaceId = useMemo<string | null>(() => {
    if (options.keyringUri === null) return null;
    const entry = workspaces.find((w) => w.headUri === options.keyringUri);
    return entry?.workspaceId ?? null;
  }, [options.keyringUri, workspaces]);

  // Replay state: when a mutation succeeds, we record what to replay
  // on fork. A chain-fork for this scope re-runs the mutation function
  // up to MAX_FORK_RETRIES times, with exponential backoff between
  // attempts. The ref pattern keeps the state stable across re-renders
  // and lets the unsubscribe in the cleanup effect see the current bus.
  const replayRef = useRef<{
    readonly mutationFn: (fm: FileManager, input: TInput) => Promise<TResult>;
    readonly input: TInput;
    attempt: number;
  } | null>(null);

  // Surfaceable lifecycle state for the chain-fork retry. Lives in
  // useState (not useRef) so changes trigger re-renders — that's the
  // whole point of exposing it. Reset on every new mutation kick so
  // exhaustion from a previous attempt doesn't linger.
  const [forkRetryState, setForkRetryState] = useState<ForkRetryState>("idle");

  const dismissForkRetry = useCallback(() => {
    setForkRetryState("idle");
  }, []);

  // Subscribe to chain-fork events for the lifetime of this hook
  // instance. Only re-runs the registered mutation if the event's
  // workspaceId matches our scope. After MAX_FORK_RETRIES failed
  // replays, we mark exhausted so the call site can render a manual-
  // retry affordance.
  useEffect(() => {
    const unsubscribe = forkBus.subscribe((event) => {
      if (!forkAffectsScope(options.keyringUri, workspaceId, event)) return;
      const replay = replayRef.current;
      if (!replay) return;
      if (replay.attempt >= MAX_FORK_RETRIES) {
        replayRef.current = null;
        setForkRetryState("exhausted");
        console.warn(
          `[opake-react] chain-fork retry exhausted after ${MAX_FORK_RETRIES} attempts`,
          { workspaceId: event.workspaceId, path: event.path },
        );
        return;
      }
      const currentAttempt = replay.attempt;
      replay.attempt = currentAttempt + 1;
      setForkRetryState("retrying");
      const { mutationFn, input } = replay;

      // Bump the React Query cache so the SDK fetches a fresh tree
      // before the retry — federation cascades resolve the current
      // chain head from the indexer on each call, so the cascade
      // rebuilds against the winning head.
      void queryClient.invalidateQueries({ queryKey: key });

      (async () => {
        try {
          await backoffSleep(currentAttempt);
          await withFileManager(cache, options.keyringUri, (fm) => mutationFn(fm, input));
          // Replay succeeded. Clear the slot AND drop back to idle —
          // but only if we're still the slot owner. A subsequent fork
          // could have fired during the backoff, kicking a fresh
          // retry; in that case the new attempt should keep
          // `retrying` until it lands or exhausts.
          if (replayRef.current === replay) {
            replayRef.current = null;
            setForkRetryState("idle");
          }
        } catch (err) {
          // Failure is logged but not surfaced as `exhausted` yet —
          // the next fork event will increment the counter and either
          // retry again or transition to exhausted.
          console.warn("[opake-react] chain-fork retry attempt failed:", err);
        }
      })().catch((err: unknown) => {
        console.warn("[opake-react] chain-fork retry orchestration failed:", err);
      });
    });
    return unsubscribe;
  }, [forkBus, options.keyringUri, workspaceId, key, cache, queryClient]);

  const mutation = useMutation<TResult, Error, TInput, MutationContext>({
    mutationFn: (input) =>
      withFileManager(cache, options.keyringUri, (fm) => options.mutationFn(fm, input)),

    onMutate: async (input) => {
      // Every new mutation kick clears stale fork-retry state. If a
      // previous attempt exhausted, the user firing a fresh mutation
      // is the implicit dismiss — keeping the banner up after they've
      // moved on would be confusing.
      setForkRetryState("idle");

      if (!options.optimisticUpdate) {
        return {};
      }
      const apply = options.optimisticUpdate;

      // Legacy queryCache path — still needed for `useTree` consumers
      // (deprecated but kept for invalidation semantics).
      await queryClient.cancelQueries({ queryKey: key });
      const previous = queryClient.getQueryData<DirectoryTreeSnapshot>(key);
      if (previous) {
        queryClient.setQueryData<DirectoryTreeSnapshot>(key, (old) =>
          old ? apply(old, input) : old,
        );
      }

      // Subscription consumers (useDirectory, which is what the live
      // UI actually renders from) read from the optimistic overlay.
      // Push the same transform there so the change appears within
      // the current render instead of waiting ~1s for the SSE echo.
      const releaseOverlay = overlay.apply(scope, (snap) => apply(snap, input));

      return { previous, releaseOverlay };
    },

    onError: (_err, _input, context) => {
      if (context?.previous) {
        queryClient.setQueryData(key, context.previous);
      }
      // Release the overlay immediately on error: there's no server-side
      // state to wait for, and leaving the patch on screen would show the
      // user a mutation that never happened.
      context?.releaseOverlay?.();
      // Drop any prior replay slot — this failed mutation isn't a
      // candidate for fork-driven retry.
      replayRef.current = null;
    },

    onSuccess: (_data, input) => {
      // Register replay state so a subsequent chain-fork event can
      // re-run the same mutation. `attempt` starts at 0; the event
      // handler increments it on each retry. Workspace-only — cabinets
      // have no contention.
      if (options.keyringUri !== null) {
        replayRef.current = {
          mutationFn: options.mutationFn,
          input,
          attempt: 0,
        };
      }
    },

    onSettled: (_data, _error, _input, context) => {
      void queryClient.invalidateQueries({ queryKey: key });
      // Metadata is keyed per-directory and useDirectoryMetadata has a
      // separate cache that doesn't observe tree mutations. Rather than
      // thread a directoryUri through every mutation signature, invalidate
      // all metadata prefixes — non-active directories are inert refetches
      // and keepPreviousData suppresses loading flicker on the active one.
      void queryClient.invalidateQueries({ queryKey: ["opake", "metadata"] });

      // Release the optimistic patch on settle. Federation cascades
      // commit synchronously inside the mutation — the SDK's await
      // resolution already implies the PDS has the write, so there's
      // no need to bridge a 2s window the way pre-federation proposals
      // did. The SSE echo arrives shortly after; useDirectory's
      // watcher updates the snapshot transparently.
      context?.releaseOverlay?.();
    },
  });

  // Compose the extended return value. We spread `mutation` first so
  // our additional fields can never be overwritten by future React
  // Query field additions — TS would catch the collision at compile
  // time and we'd address it consciously.
  return {
    ...mutation,
    forkRetryState,
    dismissForkRetry,
  };
}
