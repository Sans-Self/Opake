import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient, keepPreviousData } from "@tanstack/react-query";
import type { DocumentMetadata, DocumentMetadataResolution } from "@opake/sdk";
import { useFileManagerCache } from "../provider";
import { useFileManager } from "./use-file-manager";
import { opakeKeys } from "../keys";
import { withFileManager } from "./use-tree-mutation";

/**
 * Display-facing hydration state for one document name.
 *
 * - `resolved` — decrypted; the name lives in {@link DirectoryMetadataResult.data}.
 * - `resolving` — not resolved yet, still within the retry budget (poll ongoing).
 * - `retryable` — retry budget spent and still unresolved; a manual retry may help.
 * - `undecryptable` — this caller can never decrypt it (no key / bad envelope).
 */
export type NameHydrationState = "resolved" | "resolving" | "retryable" | "undecryptable";

export interface DirectoryMetadataResult {
  /** Resolved document metadata, keyed by URI. Undefined until the first load. */
  readonly data: Readonly<Record<string, DocumentMetadata>> | undefined;
  /** Per-URI hydration state for the requested documents. */
  readonly statuses: Readonly<Record<string, NameHydrationState>>;
  /** True while the initial resolve is in flight. */
  readonly isPending: boolean;
  /** Reset the retry budget and re-resolve. For a manual "retry" affordance. */
  readonly retry: () => void;
}

/** How many background re-resolves to attempt before parking a document at `retryable`. */
const MAX_RETRIES = 5;

/** Backoff schedule for the self-healing re-resolve, capped so it stays bounded. */
function retryDelayMs(attempt: number): number {
  return Math.min(400 * 2 ** attempt, 4000);
}

function stableUrisKey(uris: readonly string[] | undefined): string {
  if (uris === undefined) return "auto";
  return [...uris].sort((a, b) => a.localeCompare(b)).join("\n");
}

/**
 * Load document metadata for a directory's contents, resolving names for an
 * explicit set of document URIs and self-healing transient misses.
 *
 * Cabinet/workspace rows are rendered from the SSE-driven tree keeper, while
 * names are hydrated here through a separate resolve. Passing
 * `expectedDocumentUris` — the exact document URIs the rendered snapshot
 * lists — means a document the keeper already shows still resolves even when
 * a `loadTree`-derived tree hasn't caught up to it, closing the divergence
 * that previously stranded a freshly created row on a permanent "Decrypting…"
 * placeholder. A document the resolve reports as not-yet-visible is retried on
 * a bounded backoff; one it reports as undecryptable is not.
 *
 * Omit `expectedDocumentUris` to fall back to resolving whatever documents a
 * `loadTree` of the directory lists (used where no live snapshot is on hand).
 *
 * @param keyringUri - Workspace keyring URI, or null for cabinet.
 * @param directoryUri - Directory to load metadata for, or null to disable.
 * @param expectedDocumentUris - Exact document URIs to resolve. Omit to derive them.
 */
export function useDirectoryMetadata(
  keyringUri: string | null,
  directoryUri: string | null,
  expectedDocumentUris?: readonly string[],
): DirectoryMetadataResult {
  const cache = useFileManagerCache();
  const queryClient = useQueryClient();
  const { fileManager, isReady: fmReady } = useFileManager(keyringUri);

  const urisKey = stableUrisKey(expectedDocumentUris);

  // A metadata-only change (rename) leaves the document set unchanged, so the
  // URIs-in-key auto-refetch wouldn't fire. Keep the watcher-driven invalidate
  // so those still refresh.
  useEffect(() => {
    if (!fmReady || !fileManager || directoryUri === null) return;
    const watcher = fileManager.watchDirectory(directoryUri, () => {
      void queryClient.invalidateQueries({ queryKey: opakeKeys.metadata(directoryUri) });
    });
    return () => watcher.close();
  }, [fileManager, fmReady, directoryUri, queryClient]);

  const query = useQuery<Readonly<Record<string, DocumentMetadataResolution>>>({
    // The URI set is part of the key: when the snapshot adds a document, the
    // key changes and the resolve re-runs without leaning on a watcher event.
    // The base `metadata(directoryUri)` prefix is preserved so tree-mutation
    // invalidations (which target that prefix) still reach this query.
    queryKey: [...opakeKeys.metadata(directoryUri ?? ""), urisKey],
    queryFn: async () => {
      if (!directoryUri) throw new Error("no directory");
      return withFileManager(cache, keyringUri, async (fm) => {
        const uris =
          expectedDocumentUris ??
          (await fm.loadTreeWithMetadata(directoryUri)).snapshot.directories[
            directoryUri
          ]?.entries.filter((e) => e.type === "document").map((e) => e.uri) ??
          [];
        if (uris.length === 0) return {};
        return fm.resolveDocumentMetadataFor(uris);
      });
    },
    enabled: directoryUri !== null,
    placeholderData: keepPreviousData,
  });

  // Bounded self-heal: while any requested document is `retryable`, re-resolve
  // on a backoff until it lands or the budget is spent. Attempts are keyed by
  // (directory, URI set) so a new document or directory resets the budget.
  const resolutions = query.data;
  const attemptKey = `${directoryUri ?? ""}::${urisKey}`;
  const attemptsRef = useRef(0);
  // Reactive mirror of "budget spent" so `statuses` recomputes when the last
  // attempt lands and the state flips `resolving` → `retryable`.
  const [budgetSpent, setBudgetSpent] = useState(false);

  // Reset the retry budget when the directory or its document set changes —
  // a fresh target deserves a fresh budget.
  useEffect(() => {
    attemptsRef.current = 0;
    // eslint-disable-next-line react-hooks/set-state-in-effect -- reset on external key change
    setBudgetSpent(false);
  }, [attemptKey]);

  const hasRetryable = useMemo(
    () =>
      resolutions !== undefined &&
      Object.values(resolutions).some((r) => r.status === "retryable"),
    [resolutions],
  );

  const dataUpdatedAt = query.dataUpdatedAt;
  useEffect(() => {
    if (!hasRetryable) return;
    if (attemptsRef.current >= MAX_RETRIES) {
      setBudgetSpent(true);
      return;
    }
    const delay = retryDelayMs(attemptsRef.current);
    const timer = setTimeout(() => {
      attemptsRef.current += 1;
      void query.refetch();
    }, delay);
    return () => clearTimeout(timer);
    // `dataUpdatedAt` (not `resolutions`) gates re-arming: React Query's
    // structural sharing keeps the data reference stable when consecutive
    // resolves are deep-equal (a document stuck `retryable`), but the
    // timestamp still advances on every settled fetch — so each attempt
    // schedules the next until the budget is spent.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasRetryable, dataUpdatedAt, budgetSpent]);

  const retry = useCallback(() => {
    attemptsRef.current = 0;
    setBudgetSpent(false);
    void query.refetch();
  }, [query]);

  const data = useMemo<Record<string, DocumentMetadata> | undefined>(() => {
    if (resolutions === undefined) return undefined;
    return Object.fromEntries(
      Object.entries(resolutions).flatMap(([uri, resolution]) =>
        resolution.status === "resolved" ? [[uri, resolution.metadata] as const] : [],
      ),
    );
  }, [resolutions]);

  const statuses = useMemo<Record<string, NameHydrationState>>(() => {
    if (resolutions === undefined) return {};
    return Object.fromEntries(
      Object.entries(resolutions).map(([uri, resolution]): [string, NameHydrationState] => [
        uri,
        resolution.status === "resolved"
          ? "resolved"
          : resolution.status === "undecryptable"
            ? "undecryptable"
            : budgetSpent
              ? "retryable"
              : "resolving",
      ]),
    );
  }, [resolutions, budgetSpent]);

  return { data, statuses, isPending: query.isPending, retry };
}
