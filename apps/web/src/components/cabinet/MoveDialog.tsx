// Dialog for moving a file or folder to a different directory.

import { forwardRef, useCallback, useImperativeHandle, useRef, useState } from "react";
import { ArrowBendUpRightIcon, FolderIcon, HouseIcon } from "@phosphor-icons/react";
import { MODAL_TRANSITION_MS } from "@/components/ConfirmDialog";
import type { DirectoryTreeSnapshot } from "@/lib/pdsTypes";
import { useTreeSnapshot } from "./TreeSnapshotContext";

// ---------------------------------------------------------------------------
// Handle
// ---------------------------------------------------------------------------

export interface MoveDialogHandle {
  readonly show: (
    entryUri: string,
    entryName: string,
    entryKind: "file" | "folder",
    currentParent: string | null,
    disabledUris: ReadonlySet<string>,
  ) => void;
}

interface MoveDialogProps {
  readonly onMove: (entryUri: string, targetDirectoryUri: string | null) => void;
  readonly rootLabel: string;
}

// ---------------------------------------------------------------------------
// Tree node
// ---------------------------------------------------------------------------

interface TreeNodeProps {
  readonly uri: string;
  readonly name: string;
  readonly depth: number;
  readonly selectedUri: string | null;
  readonly disabledUris: ReadonlySet<string>;
  readonly currentParentUri: string | null;
  readonly snapshot: DirectoryTreeSnapshot;
  readonly onSelect: (uri: string | null) => void;
}

function TreeNode({
  uri,
  name,
  depth,
  selectedUri,
  disabledUris,
  currentParentUri,
  snapshot,
  onSelect,
}: TreeNodeProps) {
  const isDisabled = disabledUris.has(uri);
  const isSelected = selectedUri === uri;
  const isCurrent = uri === currentParentUri;

  // Child directories: entries that exist in the snapshot's directories map
  const dirEntry = snapshot.directories[uri];
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
  const childDirUris = dirEntry
    ? dirEntry.entries.filter((e) => e.type === "directory").map((e) => e.uri)
    : [];

  return (
    <li role="treeitem" aria-selected={isSelected} aria-disabled={isDisabled || undefined}>
      <button
        onClick={() => !isDisabled && onSelect(uri)}
        disabled={isDisabled}
        className={`text-ui flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left transition-colors ${
          isSelected ? "bg-accent text-accent-content" : "hover:bg-bg-hover"
        } ${isDisabled ? "cursor-not-allowed opacity-40" : "cursor-pointer"}`}
        style={{ paddingLeft: `${depth * 1.25 + 0.5}rem` }}
      >
        <FolderIcon size={15} weight={isSelected ? "fill" : "regular"} className="shrink-0" />
        <span className="truncate">{name}</span>
        {isCurrent && (
          <span className="text-caption text-text-faint ml-auto shrink-0">(current)</span>
        )}
      </button>

      {childDirUris.length > 0 && (
        <ul role="group">
          {childDirUris.map((childUri) => {
            const child = snapshot.directories[childUri];
            // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
            if (!child) return null;
            return (
              <TreeNode
                key={childUri}
                uri={childUri}
                name={child.name}
                depth={depth + 1}
                selectedUri={selectedUri}
                disabledUris={disabledUris}
                currentParentUri={currentParentUri}
                snapshot={snapshot}
                onSelect={onSelect}
              />
            );
          })}
        </ul>
      )}
    </li>
  );
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const MoveDialog = forwardRef<MoveDialogHandle, MoveDialogProps>(function MoveDialog(
  { onMove, rootLabel },
  ref,
) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [entryUri, setEntryUri] = useState<string | null>(null);
  const [entryName, setEntryName] = useState("");
  const [entryKind, setEntryKind] = useState<"file" | "folder">("file");
  const [selectedTarget, setSelectedTarget] = useState<string | null>(null);
  const [disabledUris, setDisabledUris] = useState<ReadonlySet<string>>(new Set());
  const [currentParentUri, setCurrentParentUri] = useState<string | null>(null);

  const treeSnapshot = useTreeSnapshot();

  const dismiss = useCallback(() => {
    dialogRef.current?.close();
    setTimeout(() => {
      setEntryUri(null);
      setDisabledUris(new Set());
    }, MODAL_TRANSITION_MS);
  }, []);

  useImperativeHandle(ref, () => ({
    show: (
      uri: string,
      name: string,
      kind: "file" | "folder",
      currentParent: string | null,
      disabled: ReadonlySet<string>,
    ) => {
      setEntryUri(uri);
      setEntryName(name);
      setEntryKind(kind);
      setSelectedTarget(null);
      setCurrentParentUri(currentParent);
      setDisabledUris(disabled);
      dialogRef.current?.showModal();
    },
  }));

  const handleMove = useCallback(() => {
    if (!entryUri) return;
    // selectedTarget is null for root, or a directory URI
    // Resolve null if root is selected (rootUri maps to null in the store)
    const isRoot = selectedTarget !== null && selectedTarget === treeSnapshot?.rootUri;
    const targetUri = isRoot ? null : selectedTarget;
    onMove(entryUri, targetUri);
    dismiss();
  }, [entryUri, selectedTarget, treeSnapshot, onMove, dismiss]);

  // Can move if a target is selected and it's different from current parent
  const canMove =
    selectedTarget !== null &&
    selectedTarget !== currentParentUri &&
    !(currentParentUri === null && selectedTarget === treeSnapshot?.rootUri);

  // Root directory children for the tree
  const rootDir = treeSnapshot?.rootUri ? treeSnapshot.directories[treeSnapshot.rootUri] : null;
  const rootChildren = rootDir
    ? rootDir.entries.filter((e) => e.type === "directory").map((e) => e.uri)
    : [];

  return (
    <dialog ref={dialogRef} className="modal" aria-label={`Move ${entryKind}`}>
      <div className="modal-box max-w-sm">
        <div className="flex flex-col items-center gap-3 text-center">
          <div className="bg-accent flex size-11 items-center justify-center rounded-full">
            <ArrowBendUpRightIcon size={20} className="text-accent-content" />
          </div>
          <h3 className="text-base-content text-sm font-semibold">
            Move &ldquo;{entryName}&rdquo;
          </h3>
        </div>

        {/* Directory tree */}
        <div className="border-base-300/50 mt-4 max-h-64 overflow-y-auto rounded-lg border p-1">
          <ul role="tree" aria-label="Choose destination folder">
            {/* Root */}
            <li
              role="treeitem"
              aria-selected={selectedTarget !== null && selectedTarget === treeSnapshot?.rootUri}
            >
              <button
                onClick={() => treeSnapshot?.rootUri && setSelectedTarget(treeSnapshot.rootUri)}
                className={`text-ui flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left transition-colors ${
                  selectedTarget !== null && selectedTarget === treeSnapshot?.rootUri
                    ? "bg-accent text-accent-content"
                    : "hover:bg-bg-hover"
                } cursor-pointer`}
              >
                <HouseIcon size={15} className="shrink-0" />
                <span>{rootLabel}</span>
                {currentParentUri === null && (
                  <span className="text-caption text-text-faint ml-auto shrink-0">(current)</span>
                )}
              </button>

              {treeSnapshot && rootChildren.length > 0 && (
                <ul role="group">
                  {rootChildren.map((childUri) => {
                    const child = treeSnapshot.directories[childUri];
                    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard
                    if (!child) return null;
                    return (
                      <TreeNode
                        key={childUri}
                        uri={childUri}
                        name={child.name}
                        depth={1}
                        selectedUri={selectedTarget}
                        disabledUris={disabledUris}
                        currentParentUri={currentParentUri}
                        snapshot={treeSnapshot}
                        onSelect={setSelectedTarget}
                      />
                    );
                  })}
                </ul>
              )}
            </li>
          </ul>
        </div>

        <div className="modal-action justify-center gap-2">
          <button onClick={dismiss} className="btn btn-ghost btn-sm rounded-lg text-xs">
            Cancel
          </button>
          <button
            onClick={handleMove}
            disabled={!canMove}
            className="btn btn-primary btn-sm rounded-lg text-xs"
          >
            Move here
          </button>
        </div>
      </div>
      <form method="dialog" className="modal-backdrop">
        <button aria-label="Close">close</button>
      </form>
    </dialog>
  );
});
