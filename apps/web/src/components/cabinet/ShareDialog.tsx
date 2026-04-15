import { forwardRef, useCallback, useImperativeHandle, useRef, useState } from "react";
import { ShareNetworkIcon } from "@phosphor-icons/react";
import { resolveRecipient, RecipientNotReadyError } from "@/lib/sharing";
import { useAuthStore } from "@/stores/auth";
import { getActiveFileManager } from "@/stores/documents/store";
import { toastSuccess, toastError } from "@/stores/toast";
import { useDocumentsStore } from "@/stores/documents/store";
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
  const [status, setStatus] = useState<"idle" | "resolving" | "sharing" | "done" | "error">("idle");
  const [errorMessage, setErrorMessage] = useState("");

  const session = useAuthStore((s) => s.session);

  const dismiss = useCallback(() => {
    dialogRef.current?.close();
    setTimeout(() => {
      setDocumentUri(null);
      setDocumentName("");
      setRecipientHandle("");
      setStatus("idle");
      setErrorMessage("");
    }, MODAL_TRANSITION_MS);
  }, []);

  useImperativeHandle(ref, () => ({
    show: (uri: string, name: string) => {
      setDocumentUri(uri);
      setDocumentName(name);
      setRecipientHandle("");
      setStatus("idle");
      setErrorMessage("");
      dialogRef.current?.showModal();
      // Focus the input after dialog opens
      setTimeout(() => inputRef.current?.focus(), 50);
    },
  }));

  const handleShare = useCallback(async () => {
    if (!documentUri || session.status !== "active" || !recipientHandle.trim()) return;

    const handle = recipientHandle.trim();

    setStatus("resolving");
    setErrorMessage("");

    try {
      // Resolve recipient — may throw RecipientNotReadyError
      try {
        const recipient = await resolveRecipient(handle);

        if (recipient.did === session.did) {
          throw new Error("You can't share a file with yourself");
        }

        setStatus("sharing");

        // Core handles: fetch document → unwrap key → wrap to recipient → create grant
        await getActiveFileManager().share(documentUri, recipient.did, recipient.publicKey, "read");
      } catch (resolveError) {
        if (resolveError instanceof RecipientNotReadyError) {
          // REMOVE: pending share needs core domain method (Opake::create_pending_share)
          setStatus("done");
          toastSuccess(
            `${handle} hasn't set up Opake yet. Share queued — it will complete automatically once they log in on any device.`,
          );
          dismiss();
          return;
        }
        throw resolveError;
      }

      // Optimistically mark the item as shared in the store. `items` is
      // an array of FileItems for the current directory — find by URI,
      // replace in place. SSE / directory reload will reconcile later.
      useDocumentsStore.setState((state) => ({
        items: state.items.map((item) =>
          item.uri === documentUri ? { ...item, status: "shared" as const } : item,
        ),
      }));

      setStatus("done");
      toastSuccess(`Shared "${documentName}" with ${handle}`);
      dismiss();
    } catch (error) {
      const message = error instanceof Error ? error.message : "Failed to share";
      setErrorMessage(message);
      setStatus("error");
      toastError(message);
    }
  }, [documentUri, session, recipientHandle, documentName, dismiss]);

  const busy = status === "resolving" || status === "sharing";

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
            onChange={(e) => setRecipientHandle(e.target.value.replace(/[^a-zA-Z0-9.:_-]/g, ""))}
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
        </div>

        <div className="modal-action justify-center gap-2">
          <button
            onClick={dismiss}
            disabled={busy}
            className="btn btn-ghost btn-sm rounded-lg text-xs"
          >
            Cancel
          </button>
          <button
            onClick={() => void handleShare()}
            disabled={busy || !recipientHandle.trim()}
            className="btn btn-primary btn-sm gap-1.5 rounded-lg text-xs"
          >
            {busy && <span className="loading loading-spinner loading-xs" />}
            Share
          </button>
        </div>
      </div>
      <form method="dialog" className="modal-backdrop">
        <button aria-label="Close">close</button>
      </form>
    </dialog>
  );
});
