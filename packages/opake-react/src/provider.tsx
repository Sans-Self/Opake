// OpakeProvider — React context for the Opake instance.
//
// Wraps the app with an Opake instance and optionally a QueryClient.
// Components access the instance via useOpake().

import { createContext, useContext, useRef, type ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Opake } from "@opake/sdk";

const OpakeContext = createContext<Opake | null>(null);

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

interface OpakeProviderProps {
  /** An initialized Opake instance (or Comlink proxy to one in a worker). */
  readonly opake: Opake;
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
 * Provide an Opake instance and QueryClient to the React tree.
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
export function OpakeProvider({ opake, queryClient, children }: OpakeProviderProps) {
  const defaultClient = useRef<QueryClient | null>(null);
  if (!queryClient && !defaultClient.current) {
    defaultClient.current = createDefaultQueryClient();
  }

  return (
    <QueryClientProvider client={queryClient ?? defaultClient.current!}>
      <OpakeContext value={opake}>
        {children}
      </OpakeContext>
    </QueryClientProvider>
  );
}
