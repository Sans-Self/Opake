import { createLazyFileRoute } from "@tanstack/react-router";
import { EditorView } from "@/components/cabinet/EditorView";
import { useWorkspaceStore } from "@/stores/workspace";
import { rkeyFromUri } from "@/lib/atUri";

function WorkspaceNewEditor() {
  const { rkey } = Route.useParams();
  const { directoryUri } = Route.useSearch();
  const workspace = useWorkspaceStore((s) =>
    Object.values(s.workspaces).find((w) => rkeyFromUri(w.uri) === rkey),
  );

  if (!workspace) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-base-content/40 text-sm">Workspace not found</p>
      </div>
    );
  }

  return (
    <EditorView
      mode="new"
      context={{ kind: "workspace", keyringUri: workspace.uri }}
      returnPath={`/cabinet/workspace/${rkey}`}
      directoryUri={directoryUri}
    />
  );
}

export const Route = createLazyFileRoute("/cabinet/workspace-editor/$rkey/new")({
  component: WorkspaceNewEditor,
});
