import { createLazyFileRoute } from "@tanstack/react-router";
import { useDirectory } from "@opake/react";
import { EditorView } from "@/components/cabinet/EditorView";
import { useAuthStore } from "@/stores/auth";
import { documentUri } from "@/lib/atUri";
import { documentDirectoryPathSuffix } from "@/lib/directoryTree";

function Editor() {
  const { rkey } = Route.useParams();
  const did = useAuthStore((s) => (s.session.status === "active" ? s.session.did : null));

  // Hooks must be called unconditionally — compute URI even when did is null,
  // then guard rendering below.
  const uri = did ? documentUri(did, rkey) : null;

  // Reactive tree read so the return path updates if the user arrives via
  // direct URL before the tree has decrypted (e.g. deep link into an editor).
  const { snapshot } = useDirectory(null, null);
  const pathSuffix = uri && snapshot ? documentDirectoryPathSuffix(snapshot, uri) : null;
  const returnPath = pathSuffix ? `/cabinet/files/${pathSuffix}` : "/cabinet/files";

  if (!did || !uri) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-base-content/40 text-sm">Not signed in</p>
      </div>
    );
  }

  return (
    <EditorView
      mode="edit"
      documentUri={uri}
      context={{ kind: "cabinet" }}
      returnPath={returnPath}
    />
  );
}

export const Route = createLazyFileRoute("/cabinet/editor/$rkey")({
  component: Editor,
});
