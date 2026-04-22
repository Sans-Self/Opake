import { createLazyFileRoute } from "@tanstack/react-router";
import { useWorkspaces } from "@opake/react";
import { FileView } from "@/components/cabinet/FileView";
import { rkeyFromUri } from "@/lib/atUri";

function WorkspaceIndex() {
  const { rkey } = Route.useParams();
  const { data: workspaces } = useWorkspaces();
  const workspace = workspaces.find((w) => rkeyFromUri(w.uri) === rkey);

  if (!workspace) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-base-content/40 text-sm">Workspace not found</p>
      </div>
    );
  }

  return (
    <FileView
      rootLabel={workspace.name || "Workspace"}
      pathSegments={[]}
      context={{ kind: "workspace", keyringUri: workspace.uri }}
      basePath={`/cabinet/workspace/${rkey}`}
    />
  );
}

export const Route = createLazyFileRoute("/cabinet/workspace/$rkey/")({
  component: WorkspaceIndex,
});
