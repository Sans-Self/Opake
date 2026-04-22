"use client";

// OpakeProvider — React context for the Opake instance, a shared
// FileManager cache, and automatic SSE consumer startup.
//
// Components access:
//   - The raw Opake instance via `useOpake()` (existing API)
//   - A refcounted FileManager cache via `useFileManagerCache()` (internal)
//
// On mount, the provider calls `opake.startSseConsumer()` unless
// `disableSseAutoStart` is set. This uses the indexer URL already
// stored on the Opake instance (from config). No `indexerUrl` prop
// required — matches how `requestSseToken`, `listWorkspaces`, etc.
// resolve the URL internally.
//
// FileManagerCache lifetime is tied to the provider: unmounting the
// provider disposes all cached FileManagers. A new Opake instance
// passed as a prop creates a fresh cache.

import { createContext, useContext, useEffect, useMemo, type ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Opake } from "@opake/sdk";
import { FileManagerCache } from "./file-manager-cache";

const OpakeContext = createContext<Opake | null>(null);
const FileManagerCacheContext = createContext<FileManagerCache | null>(null);

/**
 * Access the Opake instance from context.
 *
 * Must be used within an `OpakeProvider`.
 *
 * @example
 * ```tsx
 * const opake = useOpake();
 * const workspaces = await opake.listWorkspaces();
 * ```
 */
export function useOpake(): Opake {
  const opake = useContext(OpakeContext);
  if (!opake) {
    throw new Error("useOpake must be used within an OpakeProvider");
  }
  return opake;
}

/**
 * Access the shared FileManagerCache. Internal — consumers should use
 * `useFileManager(keyringUri)` instead, which handles acquire/release
 * lifecycle for you.
 *
 * @internal
 */
export function useFileManagerCache(): FileManagerCache {
  const cache = useContext(FileManagerCacheContext);
  if (!cache) {
    throw new Error("useFileManagerCache must be used within an OpakeProvider");
  }
  return cache;
}

interface OpakeProviderProps {
  /** An initialized Opake instance (or Comlink proxy to one in a worker). */
  readonly opake: Opake;
  /**
   * Disable automatic SSE consumer start. Default false: the provider
   * calls `opake.startSseConsumer()` on mount, which uses the indexer
   * URL already stored on the Opake instance from `Opake.init()`. Set
   * true for tests, or for consumers that want explicit control via
   * `useStartSseConsumer` or a manual `opake.startSseConsumer()` call.
   */
  readonly disableSseAutoStart?: boolean;
  /** Optional QueryClient — one is created if not provided. */
  readonly queryClient?: QueryClient;
  readonly children: ReactNode;
}

function createDefaultQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        retry: 1,
      },
    },
  });
}

/**
 * Provide an Opake instance, QueryClient, and FileManagerCache to the
 * React tree. Auto-starts the WASM SSE consumer on mount.
 *
 * @example
 * ```tsx
 * import { Opake } from "@opake/sdk";
 * import { OpakeProvider } from "@opake/react";
 *
 * const opake = await Opake.init();
 *
 * function App() {
 *   return (
 *     <OpakeProvider opake={opake}>
 *       <Router />
 *     </OpakeProvider>
 *   );
 * }
 * ```
 */
export function OpakeProvider({
  opake,
  disableSseAutoStart,
  queryClient,
  children,
}: OpakeProviderProps) {
  // `useMemo` is the canonical "lazy init per component instance" pattern
  // in React 19 — `useRef` with a render-time assignment reads a ref during
  // render, which react-hooks/refs rightly forbids.
  const activeClient = useMemo(() => queryClient ?? createDefaultQueryClient(), [queryClient]);

  // FileManagerCache is tied to the current opake instance. If the
  // consumer swaps Opake instances (account switch), we build a fresh
  // cache and let the old one GC naturally.
  const cache = useMemo(() => new FileManagerCache(opake), [opake]);

  // Dispose cached FileManagers when the cache is replaced or the
  // provider unmounts.
  useEffect(() => {
    return () => {
      cache.disposeAll();
    };
  }, [cache]);

  // Auto-start the WASM SSE consumer on mount; on unmount, stop the
  // stream then wipe the keepers so a previous user's ContentKeys and
  // decrypted names don't linger across account switches. Separate
  // calls (not a single shutdown) because callers that briefly lose
  // network want to stop streaming without evicting decrypted state.
  //
  // Doesn't delegate to `useStartSseConsumer` — that hook is a start-
  // only primitive with no cleanup (safe for ad-hoc consumers), while
  // the provider owns the full start+stop+wipe lifecycle bound to the
  // React tree.
  useEffect(() => {
    if (disableSseAutoStart) return;
    void opake.startSseConsumer().catch((err: unknown) => {
      console.warn("[opake-react] startSseConsumer failed:", err);
    });
    return () => {
      try {
        opake.stopSseConsumer();
      } catch (err) {
        console.warn("[opake-react] stopSseConsumer failed:", err);
      }
      try {
        opake.wipeState();
      } catch (err) {
        console.warn("[opake-react] wipeState failed:", err);
      }
    };
  }, [opake, disableSseAutoStart]);

  return (
    <QueryClientProvider client={activeClient}>
      <OpakeContext value={opake}>
        <FileManagerCacheContext value={cache}>{children}</FileManagerCacheContext>
      </OpakeContext>
    </QueryClientProvider>
  );
}
