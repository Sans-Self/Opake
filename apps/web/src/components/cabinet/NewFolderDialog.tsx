import { forwardRef, useCallback, useImperativeHandle, useMemo, useRef, useState } from "react";
import { FolderPlusIcon } from "@phosphor-icons/react";
import { MODAL_TRANSITION_MS } from "@/components/ConfirmDialog";

export interface NewFolderDialogHandle {
  readonly show: () => void;
}

/**
 * Validator runs on every keystroke after the user has typed something.
 * When omitted, only the built-in empty/whitespace check gates Create.
 *
 * Callers wire this to the surrounding name-availability check
 * (validation + uniqueness in the target parent) so collisions are
 * surfaced before the mutation fires. Returning a message also drives
 * the inline error display.
 */
type DialogValidation = { readonly ok: true } | { readonly ok: false; readonly message: string };

interface NewFolderDialogProps {
  readonly onConfirm: (name: string) => void;
  readonly validate?: (name: string) => DialogValidation;
}

export const NewFolderDialog = forwardRef<NewFolderDialogHandle, NewFolderDialogProps>(
  function NewFolderDialog({ onConfirm, validate }, ref) {
    const dialogRef = useRef<HTMLDialogElement>(null);
    const inputRef = useRef<HTMLInputElement>(null);
    const [name, setName] = useState("");

    // Hide error feedback until the user has interacted. Showing
    // "Name cannot be empty" the instant the dialog opens is noisy —
    // the disabled Create button already communicates the same thing.
    const [touched, setTouched] = useState(false);

    const trimmed = name.trim();

    const validation: DialogValidation | null = useMemo(() => {
      if (trimmed.length === 0) return null;
      if (!validate) return { ok: true };
      return validate(trimmed);
    }, [trimmed, validate]);

    const errorMessage =
      touched && validation && !validation.ok ? validation.message : null;

    const canSubmit = validation?.ok === true;

    const dismiss = useCallback(() => {
      dialogRef.current?.close();
      setTimeout(() => {
        setName("");
        setTouched(false);
      }, MODAL_TRANSITION_MS);
    }, []);

    const handleConfirm = useCallback(() => {
      if (!canSubmit) return;
      onConfirm(trimmed);
      dismiss();
    }, [canSubmit, trimmed, onConfirm, dismiss]);

    const handleKeyDown = useCallback(
      (e: React.KeyboardEvent) => {
        if (e.key === "Enter") {
          e.preventDefault();
          handleConfirm();
        }
      },
      [handleConfirm],
    );

    const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
      setName(e.target.value);
      setTouched(true);
    }, []);

    useImperativeHandle(ref, () => ({
      show: () => {
        setName("");
        setTouched(false);
        dialogRef.current?.showModal();
        // Auto-focus after modal animation
        setTimeout(() => inputRef.current?.focus(), 50);
      },
    }));

    const inputClasses = [
      "input input-bordered input-sm w-full rounded-lg text-xs",
      errorMessage ? "input-error" : "",
    ]
      .filter(Boolean)
      .join(" ");

    return (
      <dialog ref={dialogRef} className="modal" aria-label="New folder">
        <div className="modal-box max-w-sm">
          <div className="flex flex-col items-center gap-3 text-center">
            <div className="bg-accent/60 flex size-11 items-center justify-center rounded-full">
              <FolderPlusIcon size={20} className="text-primary" />
            </div>
            <h3 className="text-base-content text-sm font-semibold">New folder</h3>
            <div className="flex w-full flex-col gap-1.5">
              <input
                ref={inputRef}
                type="text"
                value={name}
                onChange={handleChange}
                onKeyDown={handleKeyDown}
                placeholder="Folder name"
                className={inputClasses}
                aria-label="Folder name"
                aria-invalid={errorMessage !== null}
                aria-describedby={errorMessage ? "new-folder-error" : undefined}
              />
              {errorMessage ? (
                <p
                  id="new-folder-error"
                  role="alert"
                  className="text-error text-left text-[11px] leading-tight"
                >
                  {errorMessage}
                </p>
              ) : null}
            </div>
          </div>
          <div className="modal-action justify-center gap-2">
            <button onClick={dismiss} className="btn btn-ghost btn-sm rounded-lg text-xs">
              Cancel
            </button>
            <button
              onClick={handleConfirm}
              disabled={!canSubmit}
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
