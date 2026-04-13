import { createLazyFileRoute } from "@tanstack/react-router";
import { EditorView } from "@/components/cabinet/EditorView";
import { useAuthStore } from "@/stores/auth";

function NewEditor() {
  const { directoryUri } = Route.useSearch();
  const did = useAuthStore((s) => (s.session.status === "active" ? s.session.did : null));

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
      returnPath="/cabinet/files"
      directoryUri={directoryUri}
    />
  );
}

export const Route = createLazyFileRoute("/cabinet/editor/new")({
  component: NewEditor,
});
