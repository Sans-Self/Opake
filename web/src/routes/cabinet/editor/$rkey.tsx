import { useCallback, useEffect, useState } from "react";
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { z } from "zod";
import { PencilSimpleIcon } from "@phosphor-icons/react";
import { MarkdownEditor } from "@/components/cabinet/MarkdownEditor";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { PanelSkeleton } from "@/components/cabinet/PanelSkeleton";
import { Breadcrumbs, BreadcrumbActive } from "@/components/cabinet/Breadcrumbs";
import { useDocumentsStore } from "@/stores/documents/store";
import { useAuthStore } from "@/stores/auth";
import { getOpakeWorker } from "@/lib/worker";
import { rkeyFromUri } from "@/lib/atUri";

const searchSchema = z.object({
  directoryUri: z.string().optional(),
});

interface LoadedDocument {
  readonly content: string;
  readonly name: string;
}

function EditEditorPage() {
  const { rkey } = Route.useParams();
  const { directoryUri } = Route.useSearch();
  const navigate = useNavigate();
  const updateContent = useDocumentsStore((s) => s.updateContent);
  const updateMetadata = useDocumentsStore((s) => s.updateMetadata);
  const saving = useDocumentsStore((s) => s.savingUri !== null);
  const ancestorsOf = useDocumentsStore((s) => s.ancestorsOf);
  const items = useDocumentsStore((s) => s.items);

  const [loaded, setLoaded] = useState<LoadedDocument | null>(null);
  const [editingName, setEditingName] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const authState = useAuthStore((s) => s.session);
  const documentUri =
    authState.status === "active" ? `at://${authState.did}/app.opake.document/${rkey}` : null;

  useEffect(() => {
    if (!documentUri || authState.status !== "active") return;

    const load = async () => {
      try {
        const worker = getOpakeWorker();
        const result = await worker.cabinetDownload(documentUri);

        const content = new TextDecoder().decode(result.plaintext);
        setLoaded({ content, name: result.filename });
      } catch (e) {
        console.error("[editor] failed to load document:", e);
        setError(e instanceof Error ? e.message : "Failed to load document");
      }
    };

    void load();
  }, [documentUri, authState]);

  const handleSave = useCallback(
    async (content: string) => {
      if (!documentUri) return;
      const plaintext = new TextEncoder().encode(content);
      await updateContent(documentUri, plaintext);
    },
    [documentUri, updateContent],
  );

  const handleClose = useCallback(() => {
    if (directoryUri) {
      void navigate({ to: "/cabinet/files/$", params: { _splat: rkeyFromUri(directoryUri) } });
    } else {
      void navigate({ to: "/cabinet/files" });
    }
  }, [directoryUri, navigate]);

  const handleRename = useCallback(
    async (newName: string) => {
      if (!documentUri || !newName.trim()) return;
      const trimmed = newName.trim();
      await updateMetadata(documentUri, { name: trimmed });
      setLoaded((prev) => (prev ? { ...prev, name: trimmed } : prev));
      setEditingName(null);
    },
    [documentUri, updateMetadata],
  );

  // Build breadcrumbs from directory ancestry
  const ancestors = ancestorsOf(directoryUri ?? null);
  const directoryItem = directoryUri ? items[directoryUri] : undefined;
  const directoryName = directoryItem?.name ?? null;

  // Build the splat path for each ancestor
  const ancestorRkeys = ancestors.map((a) => a.rkey);

  const breadcrumbs = (
    <Breadcrumbs>
      <li>
        <Link to="/cabinet/files" className="text-text-faint">
          Your Cabinet
        </Link>
      </li>
      {ancestors.map((ancestor, index) => (
        <li key={ancestor.uri}>
          <Link
            to="/cabinet/files/$"
            params={{ _splat: ancestorRkeys.slice(0, index + 1).join("/") }}
            className="text-text-faint"
          >
            {ancestor.name}
          </Link>
        </li>
      ))}
      {directoryUri && directoryName && (
        <li>
          <Link
            to="/cabinet/files/$"
            params={{ _splat: [...ancestorRkeys, rkeyFromUri(directoryUri)].join("/") }}
            className="text-text-faint"
          >
            {directoryName}
          </Link>
        </li>
      )}
      <BreadcrumbActive>
        <PencilSimpleIcon size={13} className="mr-1 inline" />
        {loaded?.name ?? "Loading…"}
      </BreadcrumbActive>
    </Breadcrumbs>
  );

  const nameInput = loaded ? (
    <div className="flex items-center gap-2">
      <span className="text-caption text-text-muted">Name:</span>
      {editingName !== null ? (
        <input
          type="text"
          value={editingName}
          onChange={(e) => setEditingName(e.target.value)}
          onBlur={() => void handleRename(editingName)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void handleRename(editingName);
            if (e.key === "Escape") setEditingName(null);
          }}
          className="input input-bordered input-sm text-ui w-48 rounded-lg"
          // eslint-disable-next-line jsx-a11y/no-autofocus -- user initiated rename
          autoFocus
        />
      ) : (
        <button
          type="button"
          onClick={() => setEditingName(loaded.name)}
          className="text-ui text-base-content hover:text-accent transition-colors"
          title="Click to rename"
        >
          {loaded.name}
        </button>
      )}
    </div>
  ) : undefined;

  const content = error ? (
    <div className="flex h-full items-center justify-center p-8">
      <div className="text-error text-sm">{error}</div>
    </div>
  ) : !loaded ? (
    <PanelSkeleton />
  ) : (
    <MarkdownEditor
      initialContent={loaded.content}
      documentName={loaded.name}
      onSave={handleSave}
      onClose={handleClose}
      saving={saving}
    />
  );

  return (
    <PanelShell
      depth={1}
      breadcrumbs={breadcrumbs}
      toolbar={nameInput}
      footer="End-to-end encrypted · Editing"
    >
      {content}
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/editor/$rkey")({
  component: EditEditorPage,
  validateSearch: searchSchema,
});
