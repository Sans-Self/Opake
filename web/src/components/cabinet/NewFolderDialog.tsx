import { forwardRef, useCallback, useImperativeHandle, useRef, useState } from "react";
import { FolderPlusIcon } from "@phosphor-icons/react";
import { MODAL_TRANSITION_MS } from "@/components/ConfirmDialog";

export interface NewFolderDialogHandle {
  readonly show: () => void;
}

interface NewFolderDialogProps {
  readonly onConfirm: (name: string) => void;
}

export const NewFolderDialog = forwardRef<NewFolderDialogHandle, NewFolderDialogProps>(
  function NewFolderDialog({ onConfirm }, ref) {
    const dialogRef = useRef<HTMLDialogElement>(null);
    const inputRef = useRef<HTMLInputElement>(null);
    const [name, setName] = useState("");

    const dismiss = useCallback(() => {
      dialogRef.current?.close();
      setTimeout(() => setName(""), MODAL_TRANSITION_MS);
    }, []);

    const handleConfirm = useCallback(() => {
      const trimmed = name.trim();
      if (trimmed.length === 0) return;
      onConfirm(trimmed);
      dismiss();
    }, [name, onConfirm, dismiss]);

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
        dialogRef.current?.showModal();
        // Auto-focus after modal animation
        setTimeout(() => inputRef.current?.focus(), 50);
      },
    }));

    return (
      <dialog ref={dialogRef} className="modal" aria-label="New folder">
        <div className="modal-box max-w-sm">
          <div className="flex flex-col items-center gap-3 text-center">
            <div className="bg-accent/60 flex size-11 items-center justify-center rounded-full">
              <FolderPlusIcon size={20} className="text-primary" />
            </div>
            <h3 className="text-base-content text-sm font-semibold">New folder</h3>
            <input
              ref={inputRef}
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Folder name"
              className="input input-bordered input-sm w-full rounded-lg text-xs"
              aria-label="Folder name"
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
  },
);
