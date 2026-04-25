import { forwardRef, useCallback, useImperativeHandle, useRef, useState } from "react";
import { GearIcon } from "@phosphor-icons/react";
import { MODAL_TRANSITION_MS } from "@/components/ConfirmDialog";

export interface WorkspaceSettingsDialogHandle {
  readonly show: (currentName: string, currentDescription: string) => void;
}

interface WorkspaceSettingsDialogProps {
  readonly onSave: (name: string, description: string) => void;
}

export const WorkspaceSettingsDialog = forwardRef<
  WorkspaceSettingsDialogHandle,
  WorkspaceSettingsDialogProps
>(function WorkspaceSettingsDialog({ onSave }, ref) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const nameInputRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");

  const dismiss = useCallback(() => {
    dialogRef.current?.close();
    setTimeout(() => {
      setName("");
      setDescription("");
    }, MODAL_TRANSITION_MS);
  }, []);

  const handleSave = useCallback(() => {
    const trimmed = name.trim();
    if (trimmed.length === 0) return;
    onSave(trimmed, description.trim());
    dismiss();
  }, [name, description, onSave, dismiss]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSave();
      }
    },
    [handleSave],
  );

  useImperativeHandle(ref, () => ({
    show: (currentName: string, currentDescription: string) => {
      setName(currentName);
      setDescription(currentDescription);
      dialogRef.current?.showModal();
      setTimeout(() => nameInputRef.current?.focus(), 50);
    },
  }));

  return (
    <dialog ref={dialogRef} className="modal" aria-label="Workspace settings">
      <div className="modal-box max-w-sm">
        <div className="flex flex-col items-center gap-3 text-center">
          <div className="bg-accent/60 flex size-11 items-center justify-center rounded-full">
            <GearIcon size={20} className="text-primary" />
          </div>
          <h3 className="text-base-content text-sm font-semibold">Workspace settings</h3>
        </div>

        <div className="mt-4 flex flex-col gap-3">
          <div>
            <label htmlFor="ws-name" className="text-caption text-text-muted mb-1 block">
              Name
            </label>
            <input
              id="ws-name"
              ref={nameInputRef}
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={handleKeyDown}
              className="input input-bordered input-sm w-full rounded-lg text-xs"
              placeholder="Workspace name"
            />
          </div>
          <div>
            <label htmlFor="ws-desc" className="text-caption text-text-muted mb-1 block">
              Description
            </label>
            <textarea
              id="ws-desc"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="textarea textarea-bordered textarea-sm w-full rounded-lg text-xs"
              placeholder="Optional description"
              rows={2}
            />
          </div>
        </div>

        <div className="modal-action justify-center gap-2">
          <button onClick={dismiss} className="btn btn-ghost btn-sm rounded-lg text-xs">
            Cancel
          </button>
          <button
            onClick={handleSave}
            disabled={name.trim().length === 0}
            className="btn btn-primary btn-sm rounded-lg text-xs"
          >
            Save
          </button>
        </div>
      </div>
      <form method="dialog" className="modal-backdrop">
        <button aria-label="Close">close</button>
      </form>
    </dialog>
  );
});
