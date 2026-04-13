import { createLazyFileRoute } from "@tanstack/react-router";
import { EditorView } from "@/components/cabinet/EditorView";
import { useAuthStore } from "@/stores/auth";
import { documentUri } from "@/lib/atUri";
import { useDocumentsStore } from "@/stores/documents/store";

function Editor() {
  const { rkey } = Route.useParams();
  const did = useAuthStore((s) => (s.session.status === "active" ? s.session.did : null));

  // Hooks must be called unconditionally — compute URI even when did is null,
  // then guard rendering below.
  const uri = did ? documentUri(did, rkey) : null;

  // Reactive: updates if the tree snapshot loads after the editor mounts
  // (e.g., direct URL navigation where the store isn't populated yet).
  const cabinetPath = useDocumentsStore((s) => (uri ? s.cabinetPathFor(uri) : null));
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
