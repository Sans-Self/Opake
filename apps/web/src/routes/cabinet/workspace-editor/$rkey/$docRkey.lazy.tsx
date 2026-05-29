import { createLazyFileRoute } from "@tanstack/react-router";
import { useDirectory, useWorkspaces } from "@opake/react";
import { EditorView } from "@/components/cabinet/EditorView";
import { useAuthStore } from "@/stores/auth";
import { rkeyFromUri } from "@/lib/atUri";
import { findDocumentUriByRkey, findParentUri } from "@/lib/directoryTree";
import { directoryNamePathSuffix } from "@/lib/namePath";

function WorkspaceEditor() {
  const { rkey, docRkey } = Route.useParams();
  const did = useAuthStore((s) => (s.session.status === "active" ? s.session.did : null));
  const { data: workspaces } = useWorkspaces();
  const workspace = workspaces.find((w) => rkeyFromUri(w.workspaceId) === rkey);

  // Workspace documents aren't owned by the current viewer — they live at
  // the uploader's DID, so we can't synthesize the full URI from `did +
  // docRkey` the way the cabinet editor does. Resolve the full URI from
  // the tree snapshot instead. Null until the snapshot arrives, which
  // triggers the loading path below.
  const { snapshot } = useDirectory(workspace?.headUri ?? null, null);
  const lookup = snapshot ? findDocumentUriByRkey(snapshot, docRkey) : null;
  const uri = lookup?.kind === "found" ? lookup.uri : null;
  const parentUri = uri && snapshot ? findParentUri(snapshot, uri) : null;
  const pathSuffix = parentUri && snapshot ? directoryNamePathSuffix(snapshot, parentUri) : null;
  const returnPath = pathSuffix
    ? `/cabinet/workspace/${rkey}/${pathSuffix}`
    : `/cabinet/workspace/${rkey}`;

  if (!did || !workspace) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-base-content/40 text-sm">
          {!did ? "Not signed in" : "Workspace not found"}
        </p>
      </div>
    );
  }

  if (!snapshot || !lookup) {
    // Still loading the tree — no UI yet.
    return null;
  }

  if (lookup.kind === "ambiguous") {
    // Multiple documents share this rkey — shouldn't happen given
    // TID-format rkeys, but `findDocumentUriByRkey` refuses to guess
    // rather than silently pick one. The console warning fires from
    // the helper; surface a distinct message so the user knows this
    // isn't an ordinary "404 — page not found" situation.
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-base-content/40 text-sm">
          Document reference is ambiguous — multiple documents share this rkey
        </p>
      </div>
    );
  }

  if (lookup.kind === "not-found" || !uri) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-base-content/40 text-sm">Document not found in this workspace</p>
      </div>
    );
  }

  return (
    <EditorView
      mode="edit"
      documentUri={uri}
      context={{ kind: "workspace", keyringUri: workspace.headUri }}
      returnPath={returnPath}
      parentDirectoryUri={parentUri}
    />
  );
}

export const Route = createLazyFileRoute("/cabinet/workspace-editor/$rkey/$docRkey")({
  component: WorkspaceEditor,
});
