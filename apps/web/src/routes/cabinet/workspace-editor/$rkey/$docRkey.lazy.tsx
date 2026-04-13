import { createLazyFileRoute } from "@tanstack/react-router";
import { EditorView } from "@/components/cabinet/EditorView";
import { useAuthStore } from "@/stores/auth";
import { useWorkspaceStore } from "@/stores/workspace";
import { documentUri, rkeyFromUri } from "@/lib/atUri";

function WorkspaceEditor() {
  const { rkey, docRkey } = Route.useParams();
  const did = useAuthStore((s) => (s.session.status === "active" ? s.session.did : null));
  const workspace = useWorkspaceStore((s) =>
    Object.values(s.workspaces).find((w) => rkeyFromUri(w.uri) === rkey),
  );

  if (!did || !workspace) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-base-content/40 text-sm">
          {!did ? "Not signed in" : "Workspace not found"}
        </p>
      </div>
    );
  }

  const uri = documentUri(did, docRkey);

  return (
    <EditorView
      mode="edit"
      documentUri={uri}
      context={{ kind: "workspace", keyringUri: workspace.uri }}
      returnPath={`/cabinet/workspace/${rkey}`}
    />
  );
}

export const Route = createLazyFileRoute("/cabinet/workspace-editor/$rkey/$docRkey")({
  component: WorkspaceEditor,
});
