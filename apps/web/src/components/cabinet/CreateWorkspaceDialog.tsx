import { forwardRef, useCallback, useImperativeHandle, useRef, useState } from "react";
import { UsersIcon } from "@phosphor-icons/react";
import { MODAL_TRANSITION_MS } from "@/components/ConfirmDialog";

export interface CreateWorkspaceDialogHandle {
  readonly show: () => void;
}

interface CreateWorkspaceDialogProps {
  readonly onConfirm: (name: string, description: string | undefined) => void;
}

export const CreateWorkspaceDialog = forwardRef<
  CreateWorkspaceDialogHandle,
  CreateWorkspaceDialogProps
>(function CreateWorkspaceDialog({ onConfirm }, ref) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");

  const dismiss = useCallback(() => {
    dialogRef.current?.close();
    setTimeout(() => {
      setName("");
      setDescription("");
    }, MODAL_TRANSITION_MS);
  }, []);

  const handleConfirm = useCallback(() => {
    const trimmedName = name.trim();
    if (trimmedName.length === 0) return;
    onConfirm(trimmedName, description.trim() || undefined);
    dismiss();
  }, [name, description, onConfirm, dismiss]);

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
      dialogRef.current?.showModal();
      setTimeout(() => inputRef.current?.focus(), 50);
    },
  }));

  return (
    <dialog ref={dialogRef} className="modal" aria-label="Create workspace">
      <div className="modal-box max-w-sm">
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
          />
          <input
            type="text"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Description (optional)"
            className="input input-bordered input-sm w-full rounded-lg text-xs"
            aria-label="Workspace description"
          />
        </div>
        <div className="modal-action justify-center gap-2">
          <button onClick={dismiss} className="btn btn-ghost btn-sm rounded-lg text-xs">
            Cancel
          </button>
          <button
            onClick={handleConfirm}
            disabled={name.trim().length === 0}
            className="btn btn-primary btn-sm rounded-lg text-xs"
          >
            Create
          </button>
        </div>
      </div>
      <form method="dialog" className="modal-backdrop">
        <button aria-label="Close">close</button>
      </form>
    </dialog>
  );
});
