import { useCallback, useState } from "react";
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { z } from "zod";
import { PencilSimpleIcon } from "@phosphor-icons/react";
import { MarkdownEditor } from "@/components/cabinet/MarkdownEditor";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { Breadcrumbs, BreadcrumbActive } from "@/components/cabinet/Breadcrumbs";
import { useDocumentsStore } from "@/stores/documents/store";
import { rkeyFromUri } from "@/lib/atUri";

const searchSchema = z.object({
  directoryUri: z.string().optional(),
});

function NewEditorPage() {
  const { directoryUri } = Route.useSearch();
  const navigate = useNavigate();
  const uploadDocument = useDocumentsStore((s) => s.uploadDocument);
  const updateContent = useDocumentsStore((s) => s.updateContent);
  const saving = useDocumentsStore((s) => s.savingUri !== null);

  const [documentUri, setDocumentUri] = useState<string | null>(null);
  const [documentName, setDocumentName] = useState("Untitled.md");

  const handleSave = useCallback(
    async (content: string) => {
      const plaintext = new TextEncoder().encode(content);

      if (documentUri) {
        await updateContent(documentUri, plaintext);
      } else {
        const uri = await uploadDocument(
          plaintext,
          documentName,
          "text/markdown",
          directoryUri ?? null,
        );
        setDocumentUri(uri);

        const rkey = rkeyFromUri(uri);
        void navigate({
          to: "/cabinet/editor/$rkey",
          params: { rkey },
          search: directoryUri ? { directoryUri } : {},
          replace: true,
        });
      }
    },
    [documentUri, documentName, directoryUri, uploadDocument, updateContent, navigate],
  );

  const handleClose = useCallback(() => {
    if (directoryUri) {
      void navigate({ to: "/cabinet/files/$", params: { _splat: rkeyFromUri(directoryUri) } });
    } else {
      void navigate({ to: "/cabinet/files" });
    }
  }, [directoryUri, navigate]);

  const breadcrumbs = (
    <Breadcrumbs>
      <li>
        <Link to="/cabinet/files" className="text-text-faint">
          Your Cabinet
        </Link>
      </li>
      <BreadcrumbActive>
        <PencilSimpleIcon size={13} className="mr-1 inline" />
        New document
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
      footer="End-to-end encrypted · New document"
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

export const Route = createFileRoute("/cabinet/editor/new")({
  component: NewEditorPage,
  validateSearch: searchSchema,
});
