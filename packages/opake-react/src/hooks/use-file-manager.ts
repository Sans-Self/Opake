"use client";

// useFileManager — acquire a shared FileManager via the provider's
// FileManagerCache. This is the primitive that long-lived subscription
// hooks (like useDirectory) build on: it gives them a FileManager that
// outlives their own mount scope and is shared with any other hook
// watching the same context.
//
// Lifecycle:
//   Mount       → cache.acquire(keyringUri) → Promise resolves → state set
//   Unmount     → cache.release(keyringUri)
//   Prop change → release old, acquire new
//
// StrictMode double-mounts are correct because acquire increments
// refcount and release decrements it; the refcount is balanced at steady
// state regardless of how many effects fired.

import { useEffect, useState } from "react";
import type { FileManager } from "@opake/sdk";
import { useFileManagerCache } from "../provider";

interface UseFileManagerResult {
  /** The resolved FileManager, or null while the promise is in-flight. */
  readonly fileManager: FileManager | null;
  /** True once the FileManager is available. */
  readonly isReady: boolean;
  /** Non-null if acquisition failed. */
  readonly error: Error | null;
}

/**
 * Acquire a shared FileManager for a cabinet or workspace context.
 *
 * Pass `null` for the cabinet, or a workspace keyring URI. The hook
 * holds a reference for its lifetime; multiple mounted hooks for the
 * same context share one underlying FileManager.
 *
 * @example
 * ```tsx
 * const { fileManager, isReady } = useFileManager(null); // cabinet
 * if (!isReady) return <Spinner />;
 * // fileManager is safe to use
 * ```
 */
// State is keyed by the `keyringUri` that produced it. When the prop
// changes, the commit is "stale" until the new promise resolves — we
// detect that by comparing `state.key` to the current prop during render
// rather than eagerly nulling state inside the effect (which would
// trigger `react-hooks/set-state-in-effect` and cascade-render).
interface Commit {
  readonly key: string | null;
  readonly fileManager: FileManager | null;
  readonly error: Error | null;
}

export function useFileManager(keyringUri: string | null): UseFileManagerResult {
  const cache = useFileManagerCache();
  const [commit, setCommit] = useState<Commit | null>(null);

  useEffect(() => {
    let cancelled = false;

    cache.acquire(keyringUri).then(
      (fm) => {
        if (!cancelled) setCommit({ key: keyringUri, fileManager: fm, error: null });
      },
      (err: unknown) => {
        if (!cancelled) {
          setCommit({ key: keyringUri, fileManager: null, error: err as Error });
        }
      },
    );

    return () => {
      cancelled = true;
      cache.release(keyringUri);
    };
  }, [cache, keyringUri]);

  // Derive: only honor commits whose key matches the current prop.
  // Otherwise we're mid-transition and callers should see "loading".
  const current = commit?.key === keyringUri ? commit : null;
  return {
    fileManager: current?.fileManager ?? null,
    isReady: current?.fileManager != null,
    error: current?.error ?? null,
  };
}
