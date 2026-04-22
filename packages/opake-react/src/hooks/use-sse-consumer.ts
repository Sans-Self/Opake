"use client";

// useStartSseConsumer — imperative, start-only SSE consumer hook.
//
// Contract: when rendered with a defined `indexerUrl`, calls
// `opake.startSseConsumer()` once. There is deliberately no stop
// branch — unmount doesn't tear down the consumer. OpakeProvider's
// built-in auto-start covers the normal "start at mount, stop at
// unmount" lifecycle; use it for that case.
//
// Reach for this hook only when you need a gate: the consumer starts
// once `indexerUrl` flips from null to a value (e.g., authenticated
// && online, feature flag on). Passing null skips the start, so
// `useStartSseConsumer(isAuthed ? undefined : null)` composes
// cleanly without violating rules-of-hooks.
//
// Idempotent: the WASM-side `sse_started` flag prevents double-spawn,
// so calling this alongside the Provider's auto-start is a no-op on
// the second call.

import { useEffect } from "react";
import { useOpake } from "../provider";

/**
 * Start the WASM SSE consumer imperatively. No corresponding stop —
 * see the module comment for why.
 *
 * Omit `indexerUrl` to use the URL stored on the Opake instance from
 * config (recommended). Pass an explicit value to override for
 * instances without stored config. Pass `null` to skip the start
 * (use when gating on a runtime condition).
 *
 * The Provider auto-starts the consumer unless `disableSseAutoStart`
 * is set, so in most apps you don't need this hook at all. Use it
 * when you want explicit control over WHEN the consumer starts
 * (e.g., only after the user has granted camera permissions, or
 * only when a feature flag is enabled).
 *
 * @example
 * ```tsx
 * function Gate() {
 *   const isAuthenticated = useAuth();
 *   useStartSseConsumer(isAuthenticated ? undefined : null);
 *   return <Outlet />;
 * }
 * ```
 */
export function useStartSseConsumer(indexerUrl?: string | null): void {
  const opake = useOpake();

  useEffect(() => {
    // Skip when explicitly nulled — lets callers opt out conditionally
    // (e.g., `useStartSseConsumer(isAuthenticated ? undefined : null)`)
    // without breaking the rules of hooks.
    if (indexerUrl === null) return;

    void opake.startSseConsumer(indexerUrl).catch((err: unknown) => {
      console.warn("[opake-react] startSseConsumer failed:", err);
    });
  }, [opake, indexerUrl]);
}
