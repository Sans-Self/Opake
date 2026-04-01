import { useCallback, useEffect, useState } from "react";
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { PencilSimpleIcon, UsersIcon } from "@phosphor-icons/react";
import { MarkdownEditor } from "@/components/cabinet/MarkdownEditor";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { PanelSkeleton } from "@/components/cabinet/PanelSkeleton";
import { Breadcrumbs, BreadcrumbActive } from "@/components/cabinet/Breadcrumbs";
import { useKeyringStore } from "@/stores/keyring";
import { useWorkspaceStore } from "@/stores/workspaceBrowser";
import { getOpakeWorker } from "@/lib/worker";
import { rkeyFromUri, didFromUri } from "@/lib/atUri";

interface LoadedDocument {
  readonly content: string;
  readonly name: string;
}

function WorkspaceEditEditorPage() {
  const { rkey, docRkey } = Route.useParams();
  const navigate = useNavigate();

  const separatorIndex = docRkey.lastIndexOf("--");
  const documentUri =
    separatorIndex !== -1
      ? `at://${docRkey.slice(0, separatorIndex)}/app.opake.document/${docRkey.slice(separatorIndex + 2)}`
      : null;

  const keyrings = useKeyringStore((s) => s.keyrings);
  const ensureGroupKey = useKeyringStore((s) => s.ensureGroupKey);
  const updateContent = useWorkspaceStore((s) => s.updateContent);
  const fileItems = useWorkspaceStore((s) => s.fileItems);

  const keyring = Object.values(keyrings).find((k) => rkeyFromUri(k.uri) === rkey);
  const keyringUri = keyring?.uri ?? null;
  const workspaceName = keyring?.name ?? "Workspace";

  const [loaded, setLoaded] = useState<LoadedDocument | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!documentUri || !keyringUri) return;

    const load = async () => {
      try {
        const groupKey = await ensureGroupKey(keyringUri);
        const worker = getOpakeWorker();
        const result = await worker.workspaceDownload(
          keyringUri,
          didFromUri(keyringUri),
          groupKey,
          BigInt(keyring?.rotation ?? 0),
          documentUri,
        );

        const content = new TextDecoder().decode(result.plaintext);
        setLoaded({ content, name: result.filename });
      } catch (e) {
        console.error("[workspace-editor] failed to load document:", e);
        setError(e instanceof Error ? e.message : "Failed to load document");
      }
    };

    void load();
  }, [documentUri, keyringUri, keyring, ensureGroupKey]);

  const handleSave = useCallback(
    async (content: string) => {
      if (!documentUri) return;
      setSaving(true);
      try {
        const plaintext = new TextEncoder().encode(content);
        await updateContent(documentUri, plaintext);
      } finally {
        setSaving(false);
      }
    },
    [documentUri, updateContent],
  );

  const handleClose = useCallback(() => {
    void navigate({ to: "/cabinet/workspace/$rkey", params: { rkey } });
  }, [navigate, rkey]);

  const existingItem = documentUri ? fileItems[documentUri] : undefined;
  const documentName = loaded?.name ?? existingItem?.name ?? "Loading…";

  const breadcrumbs = (
    <Breadcrumbs>
      <li>
        <Link to="/cabinet/workspace/$rkey" params={{ rkey }} className="text-text-faint">
          <UsersIcon size={14} className="mr-1.5 inline md:hidden" />
          {workspaceName}
        </Link>
      </li>
      <BreadcrumbActive>
        <PencilSimpleIcon size={13} className="mr-1 inline" />
        {documentName}
      </BreadcrumbActive>
    </Breadcrumbs>
  );

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
      toolbar={<span className="text-caption text-text-muted">{loaded?.name ?? "Loading…"}</span>}
      footer="End-to-end encrypted · Editing"
    >
      {content}
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/workspace-editor/$rkey/$docRkey")({
  component: WorkspaceEditEditorPage,
});
