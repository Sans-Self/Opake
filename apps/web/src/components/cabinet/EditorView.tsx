// Shared editor view for cabinet and workspace contexts.
//
// Handles two modes:
//   - "edit": load an existing document by URI, decrypt, present MarkdownEditor
//   - "new":  blank editor; first save creates the document via upload
//
// The caller (route component) passes the context and mode; this component
// manages the FileManager lifecycle, loading state, and save plumbing.
//
// FileManager acquisition goes through @opake/react's useFileManager, which
// shares the handle with any other hook watching the same context via the
// provider's refcounted FileManagerCache.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useBlocker, useNavigate } from "@tanstack/react-router";
import {
  decodePendingUploadName,
  useDirectory,
  useDirectoryMetadata,
  useFileManager,
} from "@opake/react";
import { MarkdownEditor } from "./MarkdownEditor";
import { PanelShell } from "./PanelShell";
import { toastError, toastSuccess } from "@/stores/toast";
import { loading } from "@/stores/app";
import type { FileManager } from "@opake/sdk";
import { type FileContext, keyringUriFor } from "@/lib/fileContext";
import { checkNameAvailability, describeValidationReason, validateName } from "@/lib/namePath";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface EditorViewEditProps {
  readonly mode: "edit";
  readonly documentUri: string;
  readonly context: FileContext;
  readonly returnPath: string;
  /**
   * Parent directory URI, if known. Used to subscribe to metadata updates
   * so peer renames flow into the title input. Omit to disable the sync.
   */
  readonly parentDirectoryUri?: string | null;
}

interface EditorViewNewProps {
  readonly mode: "new";
  readonly context: FileContext;
  readonly returnPath: string;
  /** Directory to place the new document in. */
  readonly directoryUri?: string;
}

type EditorViewProps = EditorViewEditProps | EditorViewNewProps;

// ---------------------------------------------------------------------------
// Hook: load document content for editing
// ---------------------------------------------------------------------------

interface DocumentContent {
  readonly content: string;
  readonly documentName: string;
}

function useDocumentContent(
  fm: FileManager | null,
  documentUri: string | null,
): { readonly loaded: DocumentContent | null; readonly error: string | null } {
  const [loaded, setLoaded] = useState<DocumentContent | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!fm || !documentUri) return;
    // eslint-disable-next-line functional/no-let -- cleanup flag for async effect
    let cancelled = false;

    void (async () => {
      const done = loading("editor-load");
      try {
        const result = await fm.download(documentUri);
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- mutated by cleanup
        if (cancelled) return;
        const text = new TextDecoder().decode(result.data);
        setLoaded({ content: text, documentName: result.filename });
      } catch (err: unknown) {
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- mutated by cleanup
        if (cancelled) return;
        setError(err instanceof Error ? err.message : "Failed to load document");
      } finally {
        done();
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [fm, documentUri]);

  return { loaded, error };
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function EditorView(props: EditorViewProps) {
  const { mode, context, returnPath } = props;
  const navigate = useNavigate();
  const { fileManager: fm } = useFileManager(keyringUriFor(context));
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);

  // Ref-based save lock: prevents double-save from rapid Cmd+S presses
  // before React re-renders the `saving` prop into MarkdownEditor.
  const saveLockRef = useRef(false);

  // For "new" mode, track the URI once the document is created so
  // subsequent saves use updateContent instead of re-uploading.
  const [createdUri, setCreatedUri] = useState<string | null>(null);
  // Current filename. Empty string until the user has picked a name or
  // an existing document's metadata has loaded. The MarkdownEditor
  // renders "Untitled note" as a placeholder for the empty case.
  const [displayName, setDisplayName] = useState("");

  const documentUri = mode === "edit" ? props.documentUri : null;
  const directoryUri = mode === "new" ? props.directoryUri : undefined;
  const parentDirectoryUri = mode === "edit" ? props.parentDirectoryUri ?? null : null;
  const persistedUri = createdUri ?? documentUri;

  const { loaded, error } = useDocumentContent(fm, documentUri);

  // Subscribe to the parent directory's metadata so peer renames propagate
  // into the title input. MarkdownEditor won't clobber the user's keystrokes
  // while they're editing — it checks document.activeElement before syncing.
  const keyringUri = keyringUriFor(context);

  // Tree snapshot for the uniqueness check. Watching null means
  // "anchor at root" — the same call FileView's index route makes.
  const { snapshot } = useDirectory(keyringUri, null);

  // Destination dir for the uniqueness check. Edit-mode docs sit in
  // `parentDirectoryUri`; new-mode docs target `directoryUri`. When the
  // caller passes nothing (toolbar "new note" at root), the document
  // lands in the workspace/cabinet root — so the check parent is the
  // snapshot's rootUri. Pre-#4 this fell to null and skipped the check
  // entirely, letting root-level duplicates slip through.
  const checkParentUri = useMemo(
    () => parentDirectoryUri ?? directoryUri ?? snapshot?.rootUri ?? null,
    [parentDirectoryUri, directoryUri, snapshot],
  );

  const { data: directoryMetadata } = useDirectoryMetadata(keyringUri, checkParentUri);
  const peerName = documentUri ? directoryMetadata?.[documentUri]?.name : undefined;

  // Sync displayName with the loaded document's name in edit mode. Two async
  // sources feed in: the initial fm.download() decrypt (loaded.documentName)
  // and live metadata updates from SSE (peerName via useDirectoryMetadata).
  // The eslint rule flags this as set-state-in-effect, but both sources are
  // external to React — the explicit allowance applies.
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- external async sources, see comment
    if (peerName) setDisplayName(peerName);
    else if (loaded?.documentName) setDisplayName(loaded.documentName);
  }, [loaded, peerName]);

  // Block in-app navigation when there are unsaved changes.
  // TanStack Router's useBlocker covers SPA navigations that
  // beforeunload doesn't catch (sidebar clicks, back within the app).
  // `withResolver` gives us proceed/reset so we can show a confirmation dialog.
  const blocker = useBlocker({
    shouldBlockFn: () => dirty,
    enableBeforeUnload: () => dirty,
    disabled: !dirty,
    withResolver: true,
  });

  const handleClose = useCallback(() => {
    void navigate({ to: returnPath as never });
  }, [navigate, returnPath]);

  const handleDirtyChange = useCallback((isDirty: boolean) => {
    setDirty(isDirty);
  }, []);

  // Generation counter so a failing rename doesn't clobber a newer rename
  // that landed while the failure was in flight. Rollback is skipped if this
  // call's generation is no longer the most recent.
  const renameGenRef = useRef(0);

  const handleRename = useCallback(
    (newName: string) => {
      // Optimistic update — the input's UI state comes from displayName
      // via the `documentName` prop, so flip it synchronously and roll
      // back on error. For "new" mode before the first save, there's
      // nothing to persist yet; the name is applied on upload.
      const previous = displayName;
      const gen = ++renameGenRef.current;
      setDisplayName(newName);
      if (!fm || !persistedUri) return;

      // Uniqueness + name validation against siblings in the parent
      // dir, excluding the current document so renaming back to its
      // existing name is a no-op rather than a self-collision. When the
      // snapshot hasn't hydrated, fall back to validate-only so trim +
      // NFC + forbidden-char rejection still land — better than waiting
      // silently for the tree.
      const check: { readonly ok: true; readonly normalized: string } | { readonly ok: false; readonly message: string } =
        checkParentUri && snapshot
          ? checkNameAvailability({
              snapshot,
              parentUri: checkParentUri,
              rawName: newName,
              documentMetadata: directoryMetadata ?? {},
              excludeUri: persistedUri,
              pendingNameResolver: decodePendingUploadName,
            })
          : ((): { readonly ok: true; readonly normalized: string } | { readonly ok: false; readonly message: string } => {
              const v = validateName(newName);
              return v.ok
                ? { ok: true, normalized: v.normalized }
                : { ok: false, message: describeValidationReason(v.reason) };
            })();
      if (!check.ok) {
        if (renameGenRef.current === gen) setDisplayName(previous);
        toastError(check.message);
        return;
      }

      // Use the normalized name on the wire so trim + NFC reach the
      // PDS, and reflect that in the optimistic title.
      if (renameGenRef.current === gen) setDisplayName(check.normalized);

      fm.updateMetadata(persistedUri, { filename: check.normalized })
        .then(() => toastSuccess("Renamed"))
        .catch((err: unknown) => {
          if (renameGenRef.current === gen) {
            setDisplayName(previous);
          }
          toastError(err instanceof Error ? err.message : "Rename failed");
        });
    },
    [fm, persistedUri, displayName, snapshot, checkParentUri, directoryMetadata],
  );

  const handleSave = useCallback(
    async (markdown: string) => {
      if (!fm || saveLockRef.current) return;
      saveLockRef.current = true;
      setSaving(true);
      const done = loading("editor-save");
      try {
        const encoded = new TextEncoder().encode(markdown);
        // Filename priority: user-set display name, then auto-derive from
        // the first heading as a fallback for brand-new docs. Once a
        // filename is on disk (edit mode OR after the first save in new
        // mode), only an explicit rename via the title input updates it
        // — content edits no longer overwrite the filename.
        const filename = displayName.trim() || deriveFilename(markdown);

        if (mode === "edit" || createdUri) {
          const uri = createdUri ?? (props as EditorViewEditProps).documentUri;
          await fm.updateContent(uri, encoded);
          toastSuccess("Saved");
        } else {
          // First save in "new" mode — pre-check the auto-derived
          // filename against the target dir's siblings. Two new notes
          // started in the same dir at the same time should not collide
          // once both finish saving. When the snapshot hasn't hydrated,
          // fall back to validate-only so the upload at least carries
          // a trimmed + NFC-normalized filename.
          const filenameCheck =
            checkParentUri && snapshot
              ? checkNameAvailability({
                  snapshot,
                  parentUri: checkParentUri,
                  rawName: filename,
                  documentMetadata: directoryMetadata ?? {},
                  pendingNameResolver: decodePendingUploadName,
                })
              : ((): { readonly ok: true; readonly normalized: string } | { readonly ok: false; readonly message: string } => {
                  const v = validateName(filename);
                  return v.ok
                    ? { ok: true, normalized: v.normalized }
                    : { ok: false, message: describeValidationReason(v.reason) };
                })();
          if (!filenameCheck.ok) {
            toastError(filenameCheck.message);
            return;
          }
          const result = await fm.upload(encoded, filenameCheck.normalized, "text/markdown", {
            directoryUri: directoryUri ?? undefined,
          });
          setCreatedUri(result.uri);
          setDisplayName(filenameCheck.normalized);
          toastSuccess("Note created");
          // Don't navigate to the edit route — that would unmount this
          // component, lose cursor position + undo history, and trigger
          // a redundant decrypt round-trip. createdUri gates subsequent
          // saves to updateContent. The URL stays on /new until the user
          // navigates away, which is fine.
        }
        setDirty(false);
      } catch (err: unknown) {
        toastError(err instanceof Error ? err.message : "Save failed");
      } finally {
        saveLockRef.current = false;
        setSaving(false);
        done();
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `props` is stable per-render; we destructure what we need
    [fm, mode, createdUri, directoryUri, displayName, snapshot, checkParentUri, directoryMetadata],
  );

  // Determine what to show
  const isNew = mode === "new";
  const ready = isNew ? fm !== null : loaded !== null;
  const initialContent = isNew ? "" : (loaded?.content ?? "");

  // Stable key to reset Tiptap when the document changes
  const editorKey = isNew ? "new" : documentUri;

  const breadcrumbs = (
    <span className="text-ui text-text-muted">{isNew ? "New note" : "Editing"}</span>
  );

  return (
    <>
      <PanelShell depth={1} breadcrumbs={breadcrumbs} footer="End-to-end encrypted">
        {error ? (
          <div className="flex flex-1 items-center justify-center p-8">
            <div className="bg-error/10 text-error rounded-lg px-4 py-3 text-sm font-medium">
              {error}
            </div>
          </div>
        ) : !ready ? (
          <EditorSkeleton />
        ) : (
          <MarkdownEditor
            key={editorKey}
            initialContent={initialContent}
            documentName={displayName}
            onSave={handleSave}
            onClose={handleClose}
            onDirtyChange={handleDirtyChange}
            onRename={handleRename}
            saving={saving}
          />
        )}
      </PanelShell>

      {blocker.status === "blocked" && <UnsavedChangesDialog blocker={blocker} />}
    </>
  );
}

// ---------------------------------------------------------------------------
// Unsaved changes dialog
// ---------------------------------------------------------------------------

interface UnsavedChangesDialogProps {
  readonly blocker: {
    readonly status: "blocked";
    readonly proceed: () => void;
    readonly reset: () => void;
  };
}

function UnsavedChangesDialog({ blocker }: UnsavedChangesDialogProps) {
  return (
    <dialog className="modal modal-open" aria-label="Unsaved changes">
      <div className="modal-box max-w-sm">
        <div className="flex flex-col items-center gap-3 text-center">
          <h3 className="text-base-content text-sm font-semibold">Unsaved changes</h3>
          <p className="text-text-muted text-xs">
            You have unsaved changes that will be lost if you leave.
          </p>
        </div>
        <div className="modal-action justify-center gap-2">
          <button onClick={blocker.reset} className="btn btn-ghost btn-sm rounded-lg text-xs">
            Keep editing
          </button>
          <button onClick={blocker.proceed} className="btn btn-error btn-sm rounded-lg text-xs">
            Discard changes
          </button>
        </div>
      </div>
      <form method="dialog" className="modal-backdrop">
        <button onClick={blocker.reset} aria-label="Close">
          close
        </button>
      </form>
    </dialog>
  );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Derive a filename from the first heading or first line of markdown. */
function deriveFilename(markdown: string): string {
  const trimmed = markdown.trim();
  if (!trimmed) return "Untitled.md";

  // Try to extract a heading (non-greedy, anchored — no backtracking risk)
  // eslint-disable-next-line sonarjs/slow-regex -- false positive: `.+` is anchored to `$` on a single line via `m` flag
  const headingMatch = /^#{1,3}\s+(.+)$/m.exec(trimmed);
  if (headingMatch?.[1]) {
    return sanitizeFilename(headingMatch[1]) + ".md";
  }

  // Fall back to first non-empty line
  const firstLine = trimmed.split("\n").find((line) => line.trim().length > 0);
  if (firstLine) {
    const truncated = firstLine.trim().slice(0, 60);
    return sanitizeFilename(truncated) + ".md";
  }

  return "Untitled.md";
}

function sanitizeFilename(raw: string): string {
  return (
    raw
      .replace(/[/\\:*?"<>|]/g, "")
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, 100) || "Untitled"
  );
}

function EditorSkeleton() {
  return (
    <div className="flex h-full flex-col">
      <div className="border-base-300/50 flex shrink-0 items-center gap-3 border-b px-4 py-2">
        <div className="skeleton size-8 rounded-md" />
        <div className="skeleton h-4 w-40 rounded" />
        <div className="flex-1" />
        <div className="skeleton h-8 w-16 rounded-lg" />
      </div>
      <div className="border-base-300/50 flex shrink-0 items-center gap-1 border-b px-4 py-1.5">
        {Array.from({ length: 8 }, (_, i) => (
          <div key={i} className="skeleton size-6 rounded" />
        ))}
      </div>
      <div className="flex-1 p-6">
        <div className="space-y-3">
          <div className="skeleton h-4 w-3/4 rounded" />
          <div className="skeleton h-4 w-1/2 rounded" />
          <div className="skeleton h-4 w-5/6 rounded" />
        </div>
      </div>
    </div>
  );
}
