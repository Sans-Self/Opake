import { createLazyFileRoute } from "@tanstack/react-router";
import { useDirectory, useWorkspaces } from "@opake/react";
import { EditorView } from "@/components/cabinet/EditorView";
import { useAuthStore } from "@/stores/auth";
import { documentUri, rkeyFromUri } from "@/lib/atUri";
import { documentDirectoryPathSuffix } from "@/lib/directoryTree";

function WorkspaceEditor() {
  const { rkey, docRkey } = Route.useParams();
  const did = useAuthStore((s) => (s.session.status === "active" ? s.session.did : null));
  const { data: workspaces } = useWorkspaces();
  const workspace = workspaces.find((w) => rkeyFromUri(w.uri) === rkey);

  // Compute URI unconditionally so the hook below runs with stable deps
  // even when the workspace isn't resolved yet. Guard rendering below.
  const uri = did ? documentUri(did, docRkey) : null;
  const { snapshot } = useDirectory(workspace?.uri ?? null, null);
  const pathSuffix = uri && snapshot ? documentDirectoryPathSuffix(snapshot, uri) : null;
  const returnPath = pathSuffix
    ? `/cabinet/workspace/${rkey}/${pathSuffix}`
    : `/cabinet/workspace/${rkey}`;

  if (!did || !workspace || !uri) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-base-content/40 text-sm">
          {!did ? "Not signed in" : "Workspace not found"}
        </p>
      </div>
    );
  }

  return (
    <EditorView
      mode="edit"
      documentUri={uri}
      context={{ kind: "workspace", keyringUri: workspace.uri }}
      returnPath={returnPath}
    />
  );
}

export const Route = createLazyFileRoute("/cabinet/workspace-editor/$rkey/$docRkey")({
  component: WorkspaceEditor,
});
