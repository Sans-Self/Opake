"use client";

// useSseConsumer — imperative SSE consumer start.
//
// Alternative to the Provider's auto-start. Useful when you want to
// gate the consumer on a runtime condition (authenticated && online,
// feature flag, etc.) rather than starting unconditionally at
// provider mount.
//
// Idempotent: calling this while another start is already in flight
// is safe — the WASM-side `sse_started` flag prevents double-spawn.
// So calling it alongside the Provider's auto-start is a no-op on
// the second call.

import { useEffect } from "react";
import { useOpake } from "../provider";

/**
 * Start the WASM SSE consumer imperatively.
 *
 * Omit `indexerUrl` to use the URL stored on the Opake instance from
 * config (recommended). Pass an explicit value to override for
 * instances without stored config.
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
 *   useSseConsumer(isAuthenticated ? undefined : null);
 *   return <Outlet />;
 * }
 * ```
 */
export function useSseConsumer(indexerUrl?: string | null): void {
  const opake = useOpake();

  useEffect(() => {
    // Skip when explicitly nulled — lets callers opt out conditionally
    // (e.g., `useSseConsumer(isAuthenticated ? undefined : null)`)
    // without breaking the rules of hooks.
    if (indexerUrl === null) return;

    void opake.startSseConsumer(indexerUrl).catch((err: unknown) => {
      console.warn("[opake-react] startSseConsumer failed:", err);
    });
  }, [opake, indexerUrl]);
}
