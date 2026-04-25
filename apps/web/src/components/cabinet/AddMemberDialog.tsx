import { forwardRef, useCallback, useImperativeHandle, useRef, useState } from "react";
import { UserPlusIcon } from "@phosphor-icons/react";
import { MODAL_TRANSITION_MS } from "@/components/ConfirmDialog";
import type { WorkspaceRole } from "@/lib/workspaceSchemas";

export interface AddMemberDialogHandle {
  readonly show: () => void;
}

interface AddMemberDialogProps {
  readonly onConfirm: (handle: string, role: WorkspaceRole) => void;
}

const ROLE_OPTIONS: readonly { readonly value: WorkspaceRole; readonly label: string }[] = [
  { value: "editor", label: "Editor" },
  { value: "viewer", label: "Viewer" },
  { value: "manager", label: "Manager" },
];

export const AddMemberDialog = forwardRef<AddMemberDialogHandle, AddMemberDialogProps>(
  function AddMemberDialog({ onConfirm }, ref) {
    const dialogRef = useRef<HTMLDialogElement>(null);
    const inputRef = useRef<HTMLInputElement>(null);
    const [handle, setHandle] = useState("");
    const [role, setRole] = useState<WorkspaceRole>("editor");

    const dismiss = useCallback(() => {
      dialogRef.current?.close();
      setTimeout(() => {
        setHandle("");
        setRole("editor");
      }, MODAL_TRANSITION_MS);
    }, []);

    const handleConfirm = useCallback(() => {
      const trimmed = handle.trim();
      if (trimmed.length === 0) return;
      onConfirm(trimmed, role);
      dismiss();
    }, [handle, role, onConfirm, dismiss]);

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
        setHandle("");
        setRole("editor");
        dialogRef.current?.showModal();
        setTimeout(() => inputRef.current?.focus(), 50);
      },
    }));

    return (
      <dialog ref={dialogRef} className="modal" aria-label="Add member">
        <div className="modal-box max-w-sm">
          <div className="flex flex-col items-center gap-3 text-center">
            <div className="bg-accent/60 flex size-11 items-center justify-center rounded-full">
              <UserPlusIcon size={20} className="text-primary" />
            </div>
            <h3 className="text-base-content text-sm font-semibold">Add member</h3>
            <input
              ref={inputRef}
              type="text"
              value={handle}
              onChange={(e) => setHandle(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Handle (e.g. alice.bsky.social)"
              className="input input-bordered input-sm w-full rounded-lg text-xs"
              aria-label="Member handle"
            />
            <div className="flex w-full gap-1.5" role="radiogroup" aria-label="Member role">
              {ROLE_OPTIONS.map((option) => (
                <button
                  key={option.value}
                  type="button"
                  role="radio"
                  aria-checked={role === option.value}
                  onClick={() => setRole(option.value)}
                  className={`btn btn-xs flex-1 rounded-lg ${
                    role === option.value ? "btn-primary" : "btn-ghost"
                  }`}
                >
                  {option.label}
                </button>
              ))}
            </div>
          </div>
          <div className="modal-action justify-center gap-2">
            <button onClick={dismiss} className="btn btn-ghost btn-sm rounded-lg text-xs">
              Cancel
            </button>
            <button
              onClick={handleConfirm}
              disabled={handle.trim().length === 0}
              className="btn btn-primary btn-sm rounded-lg text-xs"
            >
              Add
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
