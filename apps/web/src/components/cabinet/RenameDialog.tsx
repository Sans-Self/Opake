// Dialog for renaming a directory.

import { forwardRef, useCallback, useImperativeHandle, useRef, useState } from "react";
import { PencilSimpleIcon } from "@phosphor-icons/react";
import { MODAL_TRANSITION_MS } from "@/components/ConfirmDialog";

// ---------------------------------------------------------------------------
// Handle
// ---------------------------------------------------------------------------

export interface RenameDialogHandle {
  readonly show: (uri: string, currentName: string) => void;
}

interface RenameDialogProps {
  readonly onSave: (uri: string, newName: string) => void;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const RenameDialog = forwardRef<RenameDialogHandle, RenameDialogProps>(function RenameDialog(
  { onSave },
  ref,
) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [uri, setUri] = useState<string | null>(null);
  const [name, setName] = useState("");

  const dismiss = useCallback(() => {
    dialogRef.current?.close();
    setTimeout(() => setUri(null), MODAL_TRANSITION_MS);
  }, []);

  useImperativeHandle(ref, () => ({
    show: (entryUri: string, currentName: string) => {
      setUri(entryUri);
      setName(currentName);
      dialogRef.current?.showModal();
    },
  }));

  const handleSave = useCallback(() => {
    if (!uri || !name.trim()) return;
    onSave(uri, name.trim());
    dismiss();
  }, [uri, name, onSave, dismiss]);

  const canSave = name.trim().length > 0;

  return (
    <dialog ref={dialogRef} className="modal" aria-label="Rename folder">
      <div className="modal-box max-w-sm">
        <div className="flex flex-col items-center gap-3 text-center">
          <div className="bg-accent flex size-11 items-center justify-center rounded-full">
            <PencilSimpleIcon size={20} className="text-accent-content" />
          </div>
          <h3 className="text-base-content text-sm font-semibold">Rename folder</h3>
        </div>

        <div className="mt-4">
          <label className="flex flex-col gap-1">
            <span className="text-caption text-text-muted font-medium">Name</span>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && canSave) {
                  e.preventDefault();
                  handleSave();
                }
              }}
              className="input input-bordered input-sm text-ui rounded-lg"
              placeholder="Folder name"
              required
            />
          </label>
        </div>

        <div className="modal-action justify-center gap-2">
          <button onClick={dismiss} className="btn btn-ghost btn-sm rounded-lg text-xs">
            Cancel
          </button>
          <button
            onClick={handleSave}
            disabled={!canSave}
            className="btn btn-primary btn-sm rounded-lg text-xs"
          >
            Rename
          </button>
        </div>
      </div>
      <form method="dialog" className="modal-backdrop">
        <button aria-label="Close">close</button>
      </form>
    </dialog>
  );
});
