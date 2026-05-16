import { createLazyFileRoute } from "@tanstack/react-router";
import { useWorkspaces } from "@opake/react";
import { FileView } from "@/components/cabinet/FileView";
import { rkeyFromUri } from "@/lib/atUri";

function WorkspaceFiles() {
  const { rkey, _splat } = Route.useParams();
  const { data: workspaces } = useWorkspaces();
  const workspace = workspaces.find((w) => rkeyFromUri(w.workspaceId) === rkey);

  if (!workspace) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-base-content/40 text-sm">Workspace not found</p>
      </div>
    );
  }

  const segments = (_splat ?? "").split("/").filter(Boolean);

  return (
    <FileView
      rootLabel={workspace.name || "Workspace"}
      pathSegments={segments}
      context={{ kind: "workspace", keyringUri: workspace.headUri }}
      basePath={`/cabinet/workspace/${rkey}`}
    />
  );
}

export const Route = createLazyFileRoute("/cabinet/workspace/$rkey/$")({
  component: WorkspaceFiles,
});
