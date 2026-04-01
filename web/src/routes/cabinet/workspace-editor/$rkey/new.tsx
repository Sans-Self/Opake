import { useCallback, useState } from "react";
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { z } from "zod";
import { PencilSimpleIcon, UsersIcon } from "@phosphor-icons/react";
import { MarkdownEditor } from "@/components/cabinet/MarkdownEditor";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { Breadcrumbs, BreadcrumbActive } from "@/components/cabinet/Breadcrumbs";
import { useKeyringStore } from "@/stores/keyring";
import { useWorkspaceStore } from "@/stores/workspaceBrowser";
import { rkeyFromUri, didFromUri, directoryUri as buildDirectoryUri } from "@/lib/atUri";

const searchSchema = z.object({
  directoryUri: z.string().optional(),
});

function WorkspaceNewEditorPage() {
  const { rkey } = Route.useParams();
  const { directoryUri } = Route.useSearch();
  const navigate = useNavigate();
  const keyrings = useKeyringStore((s) => s.keyrings);
  const uploadToWorkspace = useWorkspaceStore((s) => s.uploadToWorkspace);
  const treeSnapshot = useWorkspaceStore((s) => s.treeSnapshot);

  const keyring = Object.values(keyrings).find((k) => rkeyFromUri(k.uri) === rkey);
  const keyringUri = keyring?.uri ?? null;
  const workspaceName = keyring?.name ?? "Workspace";

  const [documentName, setDocumentName] = useState("Untitled.md");
  const [saving, setSaving] = useState(false);

  const handleSave = useCallback(
    async (content: string) => {
      if (!keyringUri) return;
      const plaintext = new TextEncoder().encode(content);

      setSaving(true);
      try {
        const ownerDid = didFromUri(keyringUri);
        const wsRootRkey = `ws-${rkeyFromUri(keyringUri)}`;
        const targetDir =
          directoryUri ?? treeSnapshot?.root_uri ?? buildDirectoryUri(ownerDid, wsRootRkey);

        const file = new File([plaintext], documentName, { type: "text/markdown" });
        await uploadToWorkspace(file, keyringUri, targetDir);

        void navigate({
          to: "/cabinet/workspace/$rkey",
          params: { rkey },
        });
      } finally {
        setSaving(false);
      }
    },
    [documentName, keyringUri, directoryUri, treeSnapshot, uploadToWorkspace, navigate, rkey],
  );

  const handleClose = useCallback(() => {
    void navigate({ to: "/cabinet/workspace/$rkey", params: { rkey } });
  }, [navigate, rkey]);

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
        New note
      </BreadcrumbActive>
    </Breadcrumbs>
  );

  const nameInput = (
    <div className="flex items-center gap-2">
      <span className="text-caption text-text-muted">Name:</span>
      <input
        type="text"
        data-testid="doc-name"
        value={documentName}
        onChange={(e) => setDocumentName(e.target.value)}
        className="input input-bordered input-sm text-ui w-48 rounded-lg"
        placeholder="document-name.md"
      />
    </div>
  );

  return (
    <PanelShell
      depth={1}
      breadcrumbs={breadcrumbs}
      toolbar={nameInput}
      footer="End-to-end encrypted · New note"
    >
      <MarkdownEditor
        initialContent=""
        documentName={documentName}
        onSave={handleSave}
        onClose={handleClose}
        saving={saving}
      />
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/workspace-editor/$rkey/new")({
  component: WorkspaceNewEditorPage,
  validateSearch: searchSchema,
});
