import { createLazyFileRoute } from "@tanstack/react-router";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowClockwiseIcon, DownloadSimpleIcon, ShareNetworkIcon } from "@phosphor-icons/react";
import type { InboxGrant, ResolvedGrantMetadata } from "@opake/sdk";
import { useInbox } from "@opake/react";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { OutgoingSharesSection } from "@/components/cabinet/OutgoingSharesSection";
import { getOpake } from "@/stores/auth";
import { toastError, toastSuccess } from "@/stores/toast";
import { triggerBrowserDownload } from "@/lib/download";
import { formatShortDate } from "@/lib/format";

const METADATA_BATCH_SIZE = 5;

interface ResolvedEntry {
  readonly grant: InboxGrant;
  readonly metadata: ResolvedGrantMetadata | null;
  readonly ownerHandle: string | null;
  readonly status: "resolving" | "resolved" | "error";
  readonly error?: string;
}

function SharedWithMePage() {
  const { data: grants, isLoading } = useInbox();
  const [metadataByUri, setMetadataByUri] = useState<
    Readonly<Partial<Record<string, ResolvedGrantMetadata>>>
  >({});
  const [handleByDid, setHandleByDid] = useState<Readonly<Record<string, string>>>({});
  const [failedByUri, setFailedByUri] = useState<Readonly<Record<string, string>>>({});
  const [downloading, setDownloading] = useState<string | null>(null);

  // Track which grant URIs we've already kicked off a resolve for, so the
  // effect stays idempotent under StrictMode and doesn't re-fetch on every
  // render when only `metadataByUri` updates.
  const resolutionKickedRef = useRef(new Set<string>());

  // Resolve metadata + owner handles for new grants in bounded batches.
  // Batches run serially (first 5, then next 5, etc.) to avoid opening 50
  // cross-PDS fetches simultaneously on a heavy inbox.
  //
  // Failures are NOT permanently kicked — the retry button clears failedByUri
  // for a URI and removes it from resolutionKickedRef so the effect picks it up
  // again on the next render.
  //
  // Each effect invocation gets its own `cancelled` flag (same pattern as
  // settings.lazy.tsx). The cleanup marks it true so stale async batches from
  // a previous invocation drop their setState calls instead of racing with the
  // fresh run.
  useEffect(() => {
    if (isLoading) return;

    const pending = grants.filter(
      (g) => !resolutionKickedRef.current.has(g.uri) && metadataByUri[g.uri] === undefined,
    );
    if (pending.length === 0) return;

    // Mark as kicked ONLY for entries not already failed — failures stay retriable.
    pending.forEach((g) => {
      if (!failedByUri[g.uri]) resolutionKickedRef.current.add(g.uri);
    });

    const batches = Array.from(
      { length: Math.ceil(pending.length / METADATA_BATCH_SIZE) },
      (_, i) => pending.slice(i * METADATA_BATCH_SIZE, (i + 1) * METADATA_BATCH_SIZE),
    );

    // Local cancellation flag: true once this effect instance is superseded or
    // the component unmounts. Declared inside the effect so the cleanup below
    // can set it without triggering functional/immutable-data on an outer ref.
    const cancelled = { current: false };
    // Wrapped in a thunk so TypeScript doesn't narrow `cancelled.current` to
    // `false` after the first check and flag subsequent checks as always-falsy.
    const isCancelled = (): boolean => cancelled.current;

    void batches.reduce(async (prev, batch) => {
      await prev;
      if (isCancelled()) return;

      const metaResults = await Promise.allSettled(
        batch.map((g) => getOpake().resolveGrantMetadata(g.uri)),
      );
      const ownerDids = [...new Set(batch.map((g) => g.authorDid).filter((d) => !handleByDid[d]))];
      const ownerResults = await Promise.allSettled(
        ownerDids.map((did) => getOpake().resolveIdentity(did)),
      );

      if (isCancelled()) return;

      setHandleByDid((prev) => ({
        ...prev,
        ...Object.fromEntries(
          ownerDids.flatMap((did, idx) => {
            const r = ownerResults[idx];
            return r.status === "fulfilled" && r.value.handle
              ? ([[did, r.value.handle]] as const)
              : [];
          }),
        ),
      }));

      setMetadataByUri((prev) => ({
        ...prev,
        ...Object.fromEntries(
          batch.flatMap((g, idx) => {
            const r = metaResults[idx];
            return r.status === "fulfilled" ? ([[g.uri, r.value]] as const) : [];
          }),
        ),
      }));

      // Track failures and remove cleared entries. Side-effect on the kick set lives here.
      const failureEntries = batch.flatMap((g, idx) => {
        const r = metaResults[idx];
        if (r.status === "rejected") {
          resolutionKickedRef.current.delete(g.uri);
          const msg = r.reason instanceof Error ? r.reason.message : String(r.reason);
          return [[g.uri, msg] as const];
        }
        return [];
      });
      const succeededUris = new Set(
        batch.filter((_, idx) => metaResults[idx].status !== "rejected").map((g) => g.uri),
      );
      setFailedByUri((prev) => ({
        ...Object.fromEntries(Object.entries(prev).filter(([key]) => !succeededUris.has(key))),
        ...Object.fromEntries(failureEntries),
      }));
    }, Promise.resolve());

    return () => {
      cancelled.current = true;
    };
  }, [grants, isLoading, metadataByUri, failedByUri, handleByDid]);

  const retryResolution = useCallback((uri: string) => {
    resolutionKickedRef.current.delete(uri);
    setFailedByUri((prev) =>
      Object.fromEntries(Object.entries(prev).filter(([key]) => key !== uri)),
    );
  }, []);

  const entries: readonly ResolvedEntry[] = useMemo(() => {
    if (isLoading) return [];
    return grants.map((grant) => {
      const metadata = metadataByUri[grant.uri] ?? null;
      const ownerHandle = handleByDid[grant.authorDid] ?? null;
      const err = failedByUri[grant.uri];
      if (metadata) return { grant, metadata, ownerHandle, status: "resolved" as const };
      if (err) return { grant, metadata: null, ownerHandle, status: "error" as const, error: err };
      return { grant, metadata: null, ownerHandle, status: "resolving" as const };
    });
  }, [grants, isLoading, metadataByUri, failedByUri, handleByDid]);

  const handleDownload = useCallback(async (grantUri: string) => {
    setDownloading(grantUri);
    try {
      const result = await getOpake().downloadFromGrant(grantUri);
      triggerBrowserDownload(result.data, result.filename, "application/octet-stream");
      toastSuccess(`Downloaded ${result.filename}`);
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to download";
      toastError(message);
    } finally {
      setDownloading(null);
    }
  }, []);

  const breadcrumbs = (
    <div className="breadcrumbs text-ui min-w-0 flex-1 overflow-hidden">
      <ul>
        <li>
          <span className="text-base-content font-medium">Sharing</span>
        </li>
      </ul>
    </div>
  );

  return (
    <PanelShell depth={1} breadcrumbs={breadcrumbs} footer="Incoming and outgoing shares">
      <OutgoingSharesSection />

      <section className="flex flex-col">
        <div className="border-base-300/40 flex items-center justify-between border-b px-3 py-2">
          <h2 className="text-text-muted text-[11px] font-semibold tracking-wide uppercase">
            Shared with you
          </h2>
          {!isLoading && entries.length > 0 ? (
            <span className="text-text-faint text-[11px]">{entries.length}</span>
          ) : null}
        </div>

        {isLoading ? (
          <div className="flex justify-center py-8">
            <span className="loading loading-spinner loading-sm" />
          </div>
        ) : entries.length === 0 ? (
          <div className="flex flex-col items-center gap-1 py-6 text-center">
            <div className="bg-accent flex size-9 items-center justify-center rounded-lg">
              <ShareNetworkIcon size={16} className="text-text-faint" />
            </div>
            <div className="text-text-muted text-xs">Nothing shared with you yet</div>
          </div>
        ) : (
          <ul className="flex flex-col gap-px p-3">
            {entries.map((entry) => (
              <SharedRow
                key={entry.grant.uri}
                entry={entry}
                isDownloading={downloading === entry.grant.uri}
                onDownload={() => void handleDownload(entry.grant.uri)}
                onRetry={() => retryResolution(entry.grant.uri)}
              />
            ))}
          </ul>
        )}
      </section>
    </PanelShell>
  );
}

interface SharedRowProps {
  readonly entry: ResolvedEntry;
  readonly isDownloading: boolean;
  readonly onDownload: () => void;
  readonly onRetry: () => void;
}

function SharedRow({ entry, isDownloading, onDownload, onRetry }: SharedRowProps) {
  const { grant, metadata, ownerHandle, status, error } = entry;
  const displayName = metadata?.name ?? "Encrypted file";
  const ownerLabel = ownerHandle ? `@${ownerHandle}` : grant.authorDid;

  return (
    <li className="border-base-300/40 hover:bg-base-200/40 flex items-center justify-between gap-3 rounded-lg border px-3 py-2 transition-colors">
      <div className="flex min-w-0 flex-1 items-center gap-3">
        <div className="bg-accent flex size-8 shrink-0 items-center justify-center rounded-lg">
          <ShareNetworkIcon size={16} className="text-primary" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-base-content truncate text-xs font-medium">
            {status === "resolving" ? (
              <span className="text-text-faint italic">Resolving…</span>
            ) : status === "error" ? (
              <span className="text-error">Could not decrypt metadata</span>
            ) : (
              displayName
            )}
          </div>
          {status === "error" && error ? (
            <div className="text-error/70 truncate text-[11px]" title={error}>
              {error}
            </div>
          ) : (
            <div className="text-text-faint truncate text-[11px]">
              from {ownerLabel} · {formatShortDate(grant.createdAt)}
            </div>
          )}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        {status === "error" && (
          <button
            onClick={onRetry}
            className="btn btn-ghost btn-xs rounded-md"
            aria-label="Retry metadata resolution"
            title="Retry"
          >
            <ArrowClockwiseIcon size={12} />
          </button>
        )}
        <button
          onClick={onDownload}
          disabled={isDownloading || status === "error"}
          className="btn btn-ghost btn-xs gap-1.5 rounded-md"
          aria-label={`Download ${displayName}`}
        >
          {isDownloading ? (
            <span className="loading loading-spinner loading-xs" />
          ) : (
            <DownloadSimpleIcon size={12} />
          )}
          <span className="text-[11px]">Download</span>
        </button>
      </div>
    </li>
  );
}

export const Route = createLazyFileRoute("/cabinet/shared")({
  component: SharedWithMePage,
});
