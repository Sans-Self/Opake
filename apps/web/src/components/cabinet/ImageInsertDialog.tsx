import { forwardRef, useCallback, useImperativeHandle, useRef, useState } from "react";
import { ImageIcon } from "@phosphor-icons/react";
import { MODAL_TRANSITION_MS } from "@/components/ConfirmDialog";

export interface ImageInsertDialogHandle {
  readonly show: (prefill?: { url: string; alt: string }) => void;
}

interface ImageInsertDialogProps {
  readonly onInsert: (url: string, alt: string) => void;
}

export const ImageInsertDialog = forwardRef<ImageInsertDialogHandle, ImageInsertDialogProps>(
  function ImageInsertDialog({ onInsert }, ref) {
    const dialogRef = useRef<HTMLDialogElement>(null);
    const [url, setUrl] = useState("");
    const [alt, setAlt] = useState("");
    const [editing, setEditing] = useState(false);

    const dismiss = useCallback(() => {
      dialogRef.current?.close();
      setTimeout(() => {
        setUrl("");
        setAlt("");
        setEditing(false);
      }, MODAL_TRANSITION_MS);
    }, []);

    useImperativeHandle(ref, () => ({
      show: (prefill) => {
        setUrl(prefill?.url ?? "");
        setAlt(prefill?.alt ?? "");
        setEditing(!!prefill);
        dialogRef.current?.showModal();
      },
    }));

    const handleInsert = useCallback(() => {
      if (!url.trim()) return;
      onInsert(url.trim(), alt.trim());
      dismiss();
    }, [url, alt, onInsert, dismiss]);

    const canInsert = url.trim().length > 0;

    return (
      <dialog
        ref={dialogRef}
        className="modal"
        aria-label={editing ? "Edit image" : "Insert image"}
      >
        <div className="modal-box max-w-sm">
          <div className="flex flex-col items-center gap-3 text-center">
            <div className="bg-accent flex size-11 items-center justify-center rounded-full">
              <ImageIcon size={20} className="text-accent-content" />
            </div>
            <h3 className="text-base-content text-sm font-semibold">
              {editing ? "Edit image" : "Insert image"}
            </h3>
          </div>

          <div className="mt-4 flex flex-col gap-3">
            <label className="flex flex-col gap-1">
              <span className="text-caption text-text-muted font-medium">Image URL</span>
              <input
                type="url"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && canInsert) {
                    e.preventDefault();
                    handleInsert();
                  }
                }}
                className="input input-bordered input-sm text-ui rounded-lg"
                placeholder="https://..."
                required
              />
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-caption text-text-muted font-medium">Alt text</span>
              <input
                type="text"
                value={alt}
                onChange={(e) => setAlt(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && canInsert) {
                    e.preventDefault();
                    handleInsert();
                  }
                }}
                className="input input-bordered input-sm text-ui rounded-lg"
                placeholder="Description (optional)"
              />
            </label>
          </div>

          <div className="modal-action justify-center gap-2">
            <button onClick={dismiss} className="btn btn-ghost btn-sm rounded-lg text-xs">
              Cancel
            </button>
            <button
              onClick={handleInsert}
              disabled={!canInsert}
              className="btn btn-primary btn-sm rounded-lg text-xs"
            >
              {editing ? "Update" : "Insert"}
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
