import { createLazyFileRoute } from "@tanstack/react-router";
import { useDirectory } from "@opake/react";
import { EditorView } from "@/components/cabinet/EditorView";
import { useAuthStore } from "@/stores/auth";
import { documentUri, rkeyFromUri } from "@/lib/atUri";
import { ancestorsOf, findParentUri } from "@/lib/directoryTree";

function Editor() {
  const { rkey } = Route.useParams();
  const did = useAuthStore((s) => (s.session.status === "active" ? s.session.did : null));

  // Hooks must be called unconditionally — compute URI even when did is null,
  // then guard rendering below.
  const uri = did ? documentUri(did, rkey) : null;

  // Reactive tree read so the return path updates if the user arrives via
  // direct URL before the tree has decrypted (e.g. deep link into an editor).
  const { snapshot } = useDirectory(null, null);

  // eslint-disable-next-line sonarjs/cognitive-complexity -- inline is clearer than a helper
  const cabinetPath = (() => {
    if (!uri || !snapshot) return null;
    const parentUri = findParentUri(snapshot, uri);
    if (!parentUri || parentUri === snapshot.rootUri) return null;
    const ancestors = ancestorsOf(snapshot, parentUri);
    const segments = [...ancestors.map((a) => a.rkey), rkeyFromUri(parentUri)];
    return segments.join("/");
  })();
  const returnPath = cabinetPath ? `/cabinet/files/${cabinetPath}` : "/cabinet/files";

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
