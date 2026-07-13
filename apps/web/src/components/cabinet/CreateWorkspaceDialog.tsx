import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { UsersIcon } from "@phosphor-icons/react";
import { useCreateWorkspace, useWorkspaces } from "@opake/react";
import { MODAL_TRANSITION_MS } from "@/components/ConfirmDialog";
import { toastError, toastSuccess } from "@/stores/toast";

export interface CreateWorkspaceDialogHandle {
  readonly show: () => void;
}

// The workspace projection is patched only by the indexer echo/snapshot —
// there is no optimistic sidebar entry. So the dialog stays in-flight from
// the moment the write is issued until the `keyring:upsert` echo delivers
// the entry, then dismisses. This ceiling bounds only the *visibility wait*:
// the write is already accepted once creation resolves, and the echo normally
// lands in well under a second, but a lagging pipeline must not pin the dialog
// open indefinitely — past the ceiling we reassure and let the user dismiss.
const VISIBILITY_WAIT_CEILING_MS = 20_000;

type Phase =
  | { readonly kind: "idle" }
  | { readonly kind: "creating" }
  | { readonly kind: "awaiting-visibility"; readonly keyringUri: string }
  | { readonly kind: "visibility-timeout" };

export const CreateWorkspaceDialog = forwardRef<CreateWorkspaceDialogHandle>(
  function CreateWorkspaceDialog(_props, ref) {
    const dialogRef = useRef<HTMLDialogElement>(null);
    const inputRef = useRef<HTMLInputElement>(null);
    const [name, setName] = useState("");
    const [description, setDescription] = useState("");
    const [phase, setPhase] = useState<Phase>({ kind: "idle" });

    const create = useCreateWorkspace();
    const { data: workspaces } = useWorkspaces();

    const busy = phase.kind === "creating" || phase.kind === "awaiting-visibility";

    const resetAndClose = useCallback(() => {
      dialogRef.current?.close();
      setTimeout(() => {
        setName("");
        setDescription("");
        setPhase({ kind: "idle" });
      }, MODAL_TRANSITION_MS);
    }, []);

    const handleConfirm = useCallback(() => {
      const trimmedName = name.trim();
      if (trimmedName.length === 0 || busy) return;
      // Busy-latch closes the double-create window: the button is disabled
      // for the whole in-flight span, so a fast second click is a no-op.
      setPhase({ kind: "creating" });
      create
        .mutateAsync({ name: trimmedName, description: description.trim() || undefined })
        .then((result) => {
          setPhase({ kind: "awaiting-visibility", keyringUri: result.keyringUri });
        })
        .catch((err: unknown) => {
          toastError(err instanceof Error ? err.message : "Failed to create workspace");
          setPhase({ kind: "idle" });
        });
    }, [name, description, busy, create]);

    // Dismiss the moment the indexer echo lands the new entry in the keeper —
    // this is the same event that makes the workspace actionable, so the user
    // never sees a sidebar row before the indexer can answer for it.
    useEffect(() => {
      if (phase.kind !== "awaiting-visibility") return;
      if (workspaces.some((ws) => ws.workspaceId === phase.keyringUri)) {
        toastSuccess("Workspace created");
        resetAndClose();
      }
    }, [phase, workspaces, resetAndClose]);

    // Fallback: if visibility takes longer than the ceiling, stop blocking
    // and surface the honest waiting state. The write is not lost — it will
    // appear once the pipeline catches up.
    useEffect(() => {
      if (phase.kind !== "awaiting-visibility") return;
      const timer = setTimeout(
        () => setPhase({ kind: "visibility-timeout" }),
        VISIBILITY_WAIT_CEILING_MS,
      );
      return () => clearTimeout(timer);
    }, [phase]);

    const handleKeyDown = useCallback(
      (e: React.KeyboardEvent) => {
        if (e.key === "Enter") {
          e.preventDefault();
          handleConfirm();
        }
      },
      [handleConfirm],
    );

    useImperativeHandle(ref, () => ({
      show: () => {
        setName("");
        setDescription("");
        setPhase({ kind: "idle" });
        dialogRef.current?.showModal();
        setTimeout(() => inputRef.current?.focus(), 50);
      },
    }));

    const statusText =
      phase.kind === "creating"
        ? "Creating workspace…"
        : phase.kind === "awaiting-visibility"
          ? "Finishing up — waiting for it to appear in your sidebar…"
          : phase.kind === "visibility-timeout"
            ? "Workspace created. It's taking a little longer than usual to sync — it'll appear in your sidebar shortly."
            : "";

    return (
      <dialog ref={dialogRef} className="modal" aria-label="Create workspace">
        <div className="modal-box max-w-sm" aria-busy={busy}>
          <div className="flex flex-col items-center gap-3 text-center">
            <div className="bg-accent/60 flex size-11 items-center justify-center rounded-full">
              <UsersIcon size={20} className="text-primary" />
            </div>
            <h3 className="text-base-content text-sm font-semibold">New workspace</h3>
            <input
              ref={inputRef}
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Workspace name"
              className="input input-bordered input-sm w-full rounded-lg text-xs"
              aria-label="Workspace name"
              disabled={busy || phase.kind === "visibility-timeout"}
            />
            <input
              type="text"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Description (optional)"
              className="input input-bordered input-sm w-full rounded-lg text-xs"
              aria-label="Workspace description"
              disabled={busy || phase.kind === "visibility-timeout"}
            />
            {/* Live region: kept mounted so screen readers announce each
                phase transition (creating → awaiting → reassurance). */}
            <p
              role="status"
              aria-live="polite"
              className="text-base-content/70 min-h-4 text-xs"
            >
              {statusText}
            </p>
          </div>
          <div className="modal-action justify-center gap-2">
            {phase.kind === "visibility-timeout" ? (
              <button
                onClick={resetAndClose}
                className="btn btn-primary btn-sm rounded-lg text-xs"
              >
                Close
              </button>
            ) : (
              <>
                <button
                  onClick={resetAndClose}
                  disabled={busy}
                  className="btn btn-ghost btn-sm rounded-lg text-xs"
                >
                  Cancel
                </button>
                <button
                  onClick={handleConfirm}
                  disabled={name.trim().length === 0 || busy}
                  className="btn btn-primary btn-sm rounded-lg text-xs"
                >
                  {busy && <span className="loading loading-spinner loading-xs" />}
                  Create
                </button>
              </>
            )}
          </div>
        </div>
        {/* Backdrop close is suppressed while a write is in flight so the
            dialog can't be dismissed out from under its own busy state. */}
        {!busy && (
          <form method="dialog" className="modal-backdrop">
            <button aria-label="Close">close</button>
          </form>
        )}
      </dialog>
    );
  },
);
