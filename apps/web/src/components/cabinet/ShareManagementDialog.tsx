import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import { ProhibitIcon, ShareNetworkIcon } from "@phosphor-icons/react";
import type { GrantEntry } from "@opake/sdk";
import { MODAL_TRANSITION_MS } from "@/components/ConfirmDialog";
import { getActiveFileManager } from "@/stores/documents/store";
import { toastError, toastSuccess } from "@/stores/toast";

export interface ShareManagementDialogHandle {
  readonly show: (documentUri: string, documentName: string) => void;
}

export const ShareManagementDialog = forwardRef<ShareManagementDialogHandle, object>(
  function ShareManagementDialog(_props, ref) {
    const dialogRef = useRef<HTMLDialogElement>(null);
    const [documentUri, setDocumentUri] = useState<string | null>(null);
    const [documentName, setDocumentName] = useState("");
    const [shares, setShares] = useState<readonly GrantEntry[]>([]);
    const [loading, setLoading] = useState(false);
    const [revoking, setRevoking] = useState<string | null>(null);

    const dismiss = useCallback(() => {
      dialogRef.current?.close();
      setTimeout(() => {
        setDocumentUri(null);
        setDocumentName("");
        setShares([]);
      }, MODAL_TRANSITION_MS);
    }, []);

    const loadShares = useCallback(async (uri: string) => {
      setLoading(true);
      try {
        const all = await getActiveFileManager().listShares();
        setShares(all.filter((g) => g.document === uri));
      } catch (err) {
        toastError(`Failed to load shares: ${err instanceof Error ? err.message : String(err)}`);
      } finally {
        setLoading(false);
      }
    }, []);

    useEffect(() => {
      if (documentUri) void loadShares(documentUri);
    }, [documentUri, loadShares]);

    const handleRevoke = useCallback(async (grantUri: string) => {
      setRevoking(grantUri);
      try {
        await getActiveFileManager().revokeShare(grantUri);
        // Optimistic remove — SSE `grant:delete` will arrive shortly and
        // reconcile via the InboxKeeper on peer devices, but the current
        // user's dialog needs the entry gone now.
        setShares((prev) => prev.filter((g) => g.uri !== grantUri));
        toastSuccess("Access revoked");
      } catch (err) {
        toastError(`Failed to revoke: ${err instanceof Error ? err.message : String(err)}`);
      } finally {
        setRevoking(null);
      }
    }, []);

    useImperativeHandle(ref, () => ({
      show: (uri: string, name: string) => {
        setDocumentUri(uri);
        setDocumentName(name);
        setShares([]);
        dialogRef.current?.showModal();
      },
    }));

    return (
      <dialog ref={dialogRef} className="modal" aria-label="Manage sharing">
        <div className="modal-box max-w-sm">
          <div className="flex flex-col items-center gap-3 text-center">
            <div className="bg-primary/10 flex size-11 items-center justify-center rounded-full">
              <ShareNetworkIcon size={20} className="text-primary" />
            </div>
            <h3 className="text-base-content text-sm font-semibold">Manage sharing</h3>
            <p className="text-text-muted text-xs">
              Who has access to{" "}
              <span className="text-base-content font-medium">{documentName}</span>
            </p>
          </div>

          <div className="mt-4 flex flex-col gap-1">
            {loading && (
              <span className="text-caption text-text-faint py-2 text-center">Loading…</span>
            )}
            {!loading && shares.length === 0 && (
              <span className="text-caption text-text-faint py-2 text-center">
                Not shared with anyone
              </span>
            )}
            {shares.map((grant) => {
              const isRevoking = revoking === grant.uri;
              return (
                <div key={grant.uri} className="flex items-center gap-2 rounded-lg px-2.5 py-1.5">
                  <div className="flex min-w-0 flex-1 flex-col">
                    <span className="text-ui text-base-content truncate">{grant.recipient}</span>
                    <span className="text-caption text-text-faint">
                      shared {formatDate(grant.createdAt)}
                    </span>
                  </div>
                  <button
                    onClick={() => void handleRevoke(grant.uri)}
                    disabled={isRevoking}
                    className="btn btn-ghost btn-xs gap-1 rounded-lg"
                    title="Revoke access"
                    aria-label={`Revoke access for ${grant.recipient}`}
                  >
                    {isRevoking ? (
                      <span className="loading loading-spinner loading-xs" />
                    ) : (
                      <ProhibitIcon size={12} />
                    )}
                    <span className="text-[11px]">Revoke</span>
                  </button>
                </div>
              );
            })}
          </div>

          <div className="modal-action justify-center">
            <button onClick={dismiss} className="btn btn-ghost btn-sm rounded-lg text-xs">
              Close
            </button>
          </div>
        </div>
        <form method="dialog" className="modal-backdrop">
          <button aria-label="Close">close</button>
        </form>
      </dialog>
    );
  },
);

function formatDate(iso: string): string {
  if (!iso) return "recently";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "recently";
  return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}
