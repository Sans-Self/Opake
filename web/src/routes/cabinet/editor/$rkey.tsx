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
import { base64ToUint8Array } from "@/lib/encoding";
import { storage } from "@/lib/indexeddbStorage";
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
  const saving = useDocumentsStore((s) => s.savingUri !== null);

  const [loaded, setLoaded] = useState<LoadedDocument | null>(null);
  const [error, setError] = useState<string | null>(null);

  const authState = useAuthStore((s) => s.session);
  const documentUri =
    authState.status === "active" ? `at://${authState.did}/app.opake.document/${rkey}` : null;

  useEffect(() => {
    if (!documentUri || authState.status !== "active") return;

    const load = async () => {
      try {
        const { did, pdsUrl } = authState;
        const session = await storage.loadSession(did);
        const identity = await storage.loadIdentity(did);
        const privateKey = base64ToUint8Array(identity.private_key);

        const worker = getOpakeWorker();
        const result = await worker.documentDownload(pdsUrl, session, documentUri, privateKey, did);
        await storage.saveSession(did, result.session as Parameters<typeof storage.saveSession>[1]);

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

  const breadcrumbs = (
    <Breadcrumbs>
      <li>
        <Link to="/cabinet/files" className="text-text-faint">
          Your Cabinet
        </Link>
      </li>
      <BreadcrumbActive>
        <PencilSimpleIcon size={13} className="mr-1 inline" />
        {loaded?.name ?? "Loading…"}
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
    <PanelShell depth={1} breadcrumbs={breadcrumbs} footer="End-to-end encrypted · Editing">
      {content}
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/editor/$rkey")({
  component: EditEditorPage,
  validateSearch: searchSchema,
});
