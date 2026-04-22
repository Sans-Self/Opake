import { createLazyFileRoute } from "@tanstack/react-router";
import { useDirectory } from "@opake/react";
import { EditorView } from "@/components/cabinet/EditorView";
import { useAuthStore } from "@/stores/auth";
import { directoryPathSuffix } from "@/lib/directoryTree";

function NewEditor() {
  const { directoryUri } = Route.useSearch();
  const did = useAuthStore((s) => (s.session.status === "active" ? s.session.did : null));

  // Snapshot used only to resolve the destination directory URI to a URL
  // path suffix. The search param comes from the toolbar's "new note" entry
  // in FileView and identifies where the document will be created.
  const { snapshot } = useDirectory(null, null);
  const pathSuffix = directoryUri && snapshot ? directoryPathSuffix(snapshot, directoryUri) : null;
  const returnPath = pathSuffix ? `/cabinet/files/${pathSuffix}` : "/cabinet/files";

  if (!did) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-base-content/40 text-sm">Not signed in</p>
      </div>
    );
  }

  return (
    <EditorView
      mode="new"
      context={{ kind: "cabinet" }}
      returnPath={returnPath}
      directoryUri={directoryUri}
    />
  );
}

export const Route = createLazyFileRoute("/cabinet/editor/new")({
  component: NewEditor,
});
