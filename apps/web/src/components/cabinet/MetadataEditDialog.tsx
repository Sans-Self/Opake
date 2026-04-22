// Dialog for editing document metadata — name, tags, description.

import {
  forwardRef,
  useCallback,
  useImperativeHandle,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { PencilSimpleIcon, XIcon } from "@phosphor-icons/react";
import { MODAL_TRANSITION_MS } from "@/components/ConfirmDialog";
import type { MetadataChanges } from "@/lib/fileContext";
import type { FileItem } from "./types";

export type { MetadataChanges };

// ---------------------------------------------------------------------------
// Handle
// ---------------------------------------------------------------------------

export interface MetadataEditDialogHandle {
  readonly show: (item: FileItem) => void;
}

interface MetadataEditDialogProps {
  readonly onSave: (uri: string, changes: MetadataChanges) => void;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAX_TAG_LENGTH = 32;
const MAX_DESCRIPTION_LENGTH = 500;

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const MetadataEditDialog = forwardRef<MetadataEditDialogHandle, MetadataEditDialogProps>(
  function MetadataEditDialog({ onSave }, ref) {
    const dialogRef = useRef<HTMLDialogElement>(null);
    const [item, setItem] = useState<FileItem | null>(null);

    // Form state
    const [name, setName] = useState("");
    const [tags, setTags] = useState<string[]>([]);
    const [tagInput, setTagInput] = useState("");
    const [description, setDescription] = useState("");

    const dismiss = useCallback(() => {
      dialogRef.current?.close();
      setTimeout(() => {
        setItem(null);
        setTagInput("");
      }, MODAL_TRANSITION_MS);
    }, []);

    useImperativeHandle(ref, () => ({
      show: (fileItem: FileItem) => {
        setItem(fileItem);
        setName(fileItem.name);
        setTags([...fileItem.tags]);
        setDescription(fileItem.description ?? "");
        setTagInput("");
        dialogRef.current?.showModal();
      },
    }));

    const addTag = useCallback(() => {
      const trimmed = tagInput.trim().slice(0, MAX_TAG_LENGTH);
      if (trimmed && !tags.includes(trimmed)) {
        setTags((prev) => [...prev, trimmed]);
      }
      setTagInput("");
    }, [tagInput, tags]);

    const removeTag = useCallback((tag: string) => {
      setTags((prev) => prev.filter((t) => t !== tag));
    }, []);

    const handleTagKeyDown = useCallback(
      (e: KeyboardEvent<HTMLInputElement>) => {
        if (e.key === "Enter" || e.key === ",") {
          e.preventDefault();
          addTag();
        }
      },
      [addTag],
    );

    const handleSave = useCallback(() => {
      if (!item || !name.trim()) return;
      const changes: MetadataChanges = {
        name: name.trim(),
        tags: tags.length > 0 ? tags : undefined,
        description: description.trim() || undefined,
      };
      onSave(item.uri, changes);
      dismiss();
    }, [item, name, tags, description, onSave, dismiss]);

    const canSave = name.trim().length > 0;

    return (
      <dialog ref={dialogRef} className="modal" aria-label="Edit file metadata">
        <div className="modal-box max-w-sm">
          <div className="flex flex-col items-center gap-3 text-center">
            <div className="bg-accent flex size-11 items-center justify-center rounded-full">
              <PencilSimpleIcon size={20} className="text-accent-content" />
            </div>
            <h3 className="text-base-content text-sm font-semibold">Edit details</h3>
          </div>

          <div className="mt-4 flex flex-col gap-3">
            {/* Name */}
            <label className="flex flex-col gap-1">
              <span className="text-caption text-text-muted font-medium">Name</span>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="input input-bordered input-sm text-ui w-full rounded-lg"
                placeholder="File name"
                required
              />
            </label>

            {/* Tags */}
            <div className="flex flex-col gap-1">
              <span className="text-caption text-text-muted font-medium">Tags</span>
              <div className="flex flex-wrap items-center gap-1.5">
                {tags.map((tag) => (
                  <span
                    key={tag}
                    className="badge badge-sm badge-ghost text-text-faint border-base-300/50 gap-1 border"
                  >
                    {tag}
                    <button
                      onClick={() => removeTag(tag)}
                      className="hover:text-base-content"
                      aria-label={`Remove tag ${tag}`}
                    >
                      <XIcon size={10} />
                    </button>
                  </span>
                ))}
                <input
                  type="text"
                  value={tagInput}
                  onChange={(e) => setTagInput(e.target.value)}
                  onKeyDown={handleTagKeyDown}
                  onBlur={addTag}
                  className="input input-bordered input-xs w-20 rounded-md text-xs"
                  placeholder="Add tag…"
                  maxLength={MAX_TAG_LENGTH}
                />
              </div>
            </div>

            {/* Description */}
            <label className="flex flex-col gap-1">
              <span className="text-caption text-text-muted font-medium">Description</span>
              <textarea
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                className="textarea textarea-bordered textarea-sm text-ui w-full rounded-lg"
                placeholder="Optional description"
                maxLength={MAX_DESCRIPTION_LENGTH}
                rows={3}
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
              Save
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
