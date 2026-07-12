import { forwardRef, useCallback, useImperativeHandle, useRef, useState } from "react";
import { ShareNetworkIcon } from "@phosphor-icons/react";
import { useFileManager } from "@opake/react";
import { resolveRecipient, RecipientNotReadyError } from "@/lib/sharing";
import { useAuthStore } from "@/stores/auth";
import { toastSuccess, toastError } from "@/stores/toast";
import { MODAL_TRANSITION_MS } from "@/components/ConfirmDialog";

export interface ShareDialogHandle {
  readonly show: (documentUri: string, documentName: string) => void;
}

export const ShareDialog = forwardRef<ShareDialogHandle>(function ShareDialog(_, ref) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const [documentUri, setDocumentUri] = useState<string | null>(null);
  const [documentName, setDocumentName] = useState("");
  const [recipientHandle, setRecipientHandle] = useState("");
  const [status, setStatus] = useState<
    "idle" | "resolving" | "sharing" | "notReady" | "queuing" | "done" | "error"
  >("idle");
  const [errorMessage, setErrorMessage] = useState("");
  // The recipient input as entered when resolution said RecipientNotReady —
  // queuing must target exactly what the user confirmed the warning for.
  const [notReadyRecipient, setNotReadyRecipient] = useState<string | null>(null);

  const session = useAuthStore((s) => s.session);
  // Sharing is cabinet-only (gated upstream via allowSharing).
  const { fileManager } = useFileManager(null);

  const dismiss = useCallback(() => {
    dialogRef.current?.close();
    setTimeout(() => {
      setDocumentUri(null);
      setDocumentName("");
      setRecipientHandle("");
      setStatus("idle");
      setErrorMessage("");
      setNotReadyRecipient(null);
    }, MODAL_TRANSITION_MS);
  }, []);

  useImperativeHandle(ref, () => ({
    show: (uri: string, name: string) => {
      setDocumentUri(uri);
      setDocumentName(name);
      setRecipientHandle("");
      setStatus("idle");
      setErrorMessage("");
      setNotReadyRecipient(null);
      dialogRef.current?.showModal();
      // Focus the input after dialog opens
      setTimeout(() => inputRef.current?.focus(), 50);
    },
  }));

  const handleShare = useCallback(async () => {
    if (!documentUri || session.status !== "active" || !recipientHandle.trim() || !fileManager)
      return;

    const recipient = recipientHandle.trim();

    setStatus("resolving");
    setErrorMessage("");

    try {
      // Resolve recipient — may throw RecipientNotReadyError
      try {
        const resolved = await resolveRecipient(recipient);

        if (resolved.did === session.did) {
          throw new Error("You can't share a file with yourself");
        }

        setStatus("sharing");

        // Core handles: fetch document → unwrap key → wrap to recipient → create grant
        await fileManager.share(
          documentUri,
          resolved.did,
          resolved.x25519PublicKey,
          resolved.mlKemPublicKey,
          "read",
        );
      } catch (resolveError) {
        if (resolveError instanceof RecipientNotReadyError) {
          // Never queue silently: surface the warning and let queuing be an
          // explicit second step.
          // spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
          setNotReadyRecipient(recipient);
          setStatus("notReady");
          return;
        }
        throw resolveError;
      }

      // No optimistic UI update — the SSE `grant:upsert` echo from the indexer
      // lands within a firehose round-trip (~1s) and the tree's status badge
      // reconciles via useDirectory's watcher. Any prior store-level optimistic
      // setState relied on a singleton `items` array that no longer exists.
      setStatus("done");
      toastSuccess(`Shared "${documentName}" with ${recipient}`);
      dismiss();
    } catch (error) {
      const message = error instanceof Error ? error.message : "Failed to share";
      setErrorMessage(message);
      setStatus("error");
      toastError(message);
    }
  }, [documentUri, session, recipientHandle, documentName, fileManager, dismiss]);

  const handleQueueShare = useCallback(async () => {
    if (!documentUri || !notReadyRecipient || !fileManager) return;

    setStatus("queuing");
    try {
      await fileManager.createPendingShare(documentUri, notReadyRecipient, "read", null);
      setStatus("done");
      toastSuccess(
        `Share queued for ${notReadyRecipient} — completes automatically once they set up Opake (expires in 7 days).`,
      );
      dismiss();
    } catch (error) {
      const message = error instanceof Error ? error.message : "Failed to queue share";
      setErrorMessage(message);
      setStatus("error");
      toastError(message);
    }
  }, [documentUri, notReadyRecipient, fileManager, dismiss]);

  const busy = status === "resolving" || status === "sharing" || status === "queuing";

  return (
    <dialog ref={dialogRef} className="modal" aria-label="Share file">
      <div className="modal-box max-w-sm">
        <div className="flex flex-col items-center gap-3 text-center">
          <div className="bg-primary/10 flex size-11 items-center justify-center rounded-full">
            <ShareNetworkIcon size={20} className="text-primary" />
          </div>
          <h3 className="text-base-content text-sm font-semibold">Share file</h3>
          <p className="text-text-muted text-xs">
            Share <span className="text-base-content font-medium">{documentName}</span> with another
            Opake user.
          </p>
        </div>

        <div className="mt-4">
          <label className="label" htmlFor="share-recipient">
            <span className="text-text-muted text-xs">Recipient handle</span>
          </label>
          <input
            ref={inputRef}
            id="share-recipient"
            type="text"
            placeholder="alice.bsky.social"
            value={recipientHandle}
            onChange={(e) => {
              setRecipientHandle(e.target.value.replace(/[^a-zA-Z0-9.:_-]/g, ""));
              // Editing the recipient invalidates a shown not-ready warning —
              // queuing must never target a handle the user has since changed.
              if (status === "notReady") {
                setStatus("idle");
                setNotReadyRecipient(null);
              }
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !busy && recipientHandle.trim()) {
                void handleShare();
              }
            }}
            disabled={busy}
            className="input input-bordered input-sm border-base-300/50 bg-base-200/50 text-ui w-full"
            aria-describedby={errorMessage ? "share-error" : undefined}
          />
          {errorMessage && (
            <p id="share-error" className="text-error mt-1 text-xs" role="alert">
              {errorMessage}
            </p>
          )}
          {status === "resolving" && (
            <p className="text-text-muted mt-1 text-xs">Resolving recipient…</p>
          )}
          {status === "sharing" && <p className="text-text-muted mt-1 text-xs">Creating grant…</p>}
          {(status === "notReady" || status === "queuing") && notReadyRecipient && (
            <div className="alert alert-warning mt-3 items-start gap-2 rounded-lg p-3" role="alert">
              <p className="text-xs">
                <span className="font-medium">{notReadyRecipient}</span> hasn't set up Opake yet,
                so they can't receive this share until they publish an encryption key. You can
                queue the share — it completes automatically once they join and expires after 7
                days.
              </p>
            </div>
          )}
        </div>

        <div className="modal-action justify-center gap-2">
          <button
            onClick={dismiss}
            disabled={busy}
            className="btn btn-ghost btn-sm rounded-lg text-xs"
          >
            Cancel
          </button>
          {status === "notReady" || status === "queuing" ? (
            <button
              onClick={() => void handleQueueShare()}
              disabled={busy}
              className="btn btn-warning btn-sm gap-1.5 rounded-lg text-xs"
            >
              {busy && <span className="loading loading-spinner loading-xs" />}
              Queue share
            </button>
          ) : (
            <button
              onClick={() => void handleShare()}
              disabled={busy || !recipientHandle.trim()}
              className="btn btn-primary btn-sm gap-1.5 rounded-lg text-xs"
            >
              {busy && <span className="loading loading-spinner loading-xs" />}
              Share
            </button>
          )}
        </div>
      </div>
      <form method="dialog" className="modal-backdrop">
        <button aria-label="Close">close</button>
      </form>
    </dialog>
  );
});
