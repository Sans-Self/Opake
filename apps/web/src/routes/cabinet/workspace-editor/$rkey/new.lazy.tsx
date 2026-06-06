import { createLazyFileRoute } from "@tanstack/react-router";
import { useDirectory, useWorkspaces } from "@opake/react";
import { EditorView } from "@/components/cabinet/EditorView";
import { rkeyFromUri } from "@/lib/atUri";
import { directoryNamePathSuffix } from "@/lib/namePath";

function WorkspaceNewEditor() {
  const { rkey } = Route.useParams();
  const { directoryUri } = Route.useSearch();
  const { data: workspaces } = useWorkspaces();
  const workspace = workspaces.find((w) => rkeyFromUri(w.workspaceId) === rkey);

  const { snapshot } = useDirectory(workspace?.headUri ?? null, null);
  const pathSuffix = directoryUri && snapshot ? directoryNamePathSuffix(snapshot, directoryUri) : null;
  const returnPath = pathSuffix
    ? `/cabinet/workspace/${rkey}/${pathSuffix}`
    : `/cabinet/workspace/${rkey}`;

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
      context={{ kind: "workspace", keyringUri: workspace.headUri, workspaceId: workspace.workspaceId }}
      returnPath={returnPath}
      directoryUri={directoryUri}
    />
  );
}

export const Route = createLazyFileRoute("/cabinet/workspace-editor/$rkey/new")({
  component: WorkspaceNewEditor,
});
