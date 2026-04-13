// Shared editor view for cabinet and workspace contexts.
//
// Handles two modes:
//   - "edit": load an existing document by URI, decrypt, present MarkdownEditor
//   - "new":  blank editor; first save creates the document via upload
//
// The caller (route component) passes the context and mode; this component
// manages the FileManager lifecycle, loading state, and save plumbing.
//
// The editor owns its own FileManager independent of the documents store's
// activeManager. Both hold Rc clones into the same WASM mutex — safe, but
// long saves queue behind watcher operations and vice versa.

import { useCallback, useEffect, useRef, useState } from "react";
import { useBlocker, useNavigate } from "@tanstack/react-router";
import { MarkdownEditor } from "./MarkdownEditor";
import { PanelShell } from "./PanelShell";
import { getOpake } from "@/stores/auth";
import { toastError, toastSuccess } from "@/stores/toast";
import { loading } from "@/stores/app";
import type { FileManager } from "@opake/sdk";
import type { FileContext } from "@/stores/documents/store";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface EditorViewEditProps {
  readonly mode: "edit";
  readonly documentUri: string;
  readonly context: FileContext;
  readonly returnPath: string;
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
// Hook: acquire a FileManager for the given context
// ---------------------------------------------------------------------------

function useFileManagerForContext(context: FileContext): FileManager | null {
  const [fm, setFm] = useState<FileManager | null>(null);

  // Stable key for effect deps
  const contextKey = context.kind === "workspace" ? `workspace:${context.keyringUri}` : "cabinet";

  useEffect(() => {
    // eslint-disable-next-line functional/no-let -- cleanup flag for async effect
    let disposed = false;
    // eslint-disable-next-line functional/no-let, functional/prefer-immutable-types -- need to capture for cleanup
    let handle: FileManager | null = null;

    void (async () => {
      try {
        const opake = getOpake();
        handle =
          context.kind === "cabinet"
            ? await opake.cabinet()
            : await opake.workspace(context.keyringUri);
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- mutated by cleanup
        if (!disposed) setFm(handle);
      } catch (err: unknown) {
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- mutated by cleanup
        if (!disposed) {
          toastError(err instanceof Error ? err.message : "Failed to open context");
        }
      }
    })();

    return () => {
      disposed = true;
      handle?.dispose();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- contextKey is the stable representation
  }, [contextKey]);

  return fm;
}

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
  const fm = useFileManagerForContext(context);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);

  // Ref-based save lock: prevents double-save from rapid Cmd+S presses
  // before React re-renders the `saving` prop into MarkdownEditor.
  const saveLockRef = useRef(false);

  // For "new" mode, track the URI once the document is created so
  // subsequent saves use updateContent instead of re-uploading.
  const [createdUri, setCreatedUri] = useState<string | null>(null);
  // Track the document name for new documents (set after first save)
  const [newDocName, setNewDocName] = useState<string | null>(null);

  const documentUri = mode === "edit" ? props.documentUri : null;
  const directoryUri = mode === "new" ? props.directoryUri : undefined;

  const { loaded, error } = useDocumentContent(fm, documentUri);

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

  const handleSave = useCallback(
    async (markdown: string) => {
      if (!fm || saveLockRef.current) return;
      saveLockRef.current = true;
      setSaving(true);
      const done = loading("editor-save");
      try {
        const encoded = new TextEncoder().encode(markdown);
        const filename = deriveFilename(markdown);

        if (mode === "edit" || createdUri) {
          // Update existing document content + sync filename from heading
          const uri = createdUri ?? (props as EditorViewEditProps).documentUri;
          await fm.updateContent(uri, encoded);
          await fm.updateMetadata(uri, { filename }).catch(() => {
            // Non-fatal: content saved, metadata rename failed silently.
            // The document is still accessible under its original name.
          });
          toastSuccess("Saved");
        } else {
          // Create new document via upload
          const result = await fm.upload(encoded, filename, "text/markdown", {
            directoryUri: directoryUri ?? undefined,
          });
          setCreatedUri(result.uri);
          setNewDocName(filename);
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
    [fm, mode, createdUri, directoryUri],
  );

  // Determine what to show
  const isNew = mode === "new";
  const ready = isNew ? fm !== null : loaded !== null;
  const initialContent = isNew ? "" : (loaded?.content ?? "");
  const documentName = isNew ? (newDocName ?? "Untitled note") : (loaded?.documentName ?? "");

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
            documentName={documentName}
            onSave={handleSave}
            onClose={handleClose}
            onDirtyChange={handleDirtyChange}
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
