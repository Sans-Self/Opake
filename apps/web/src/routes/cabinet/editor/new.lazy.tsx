import { createLazyFileRoute } from "@tanstack/react-router";

function NewEditor() {
  return (
    <div className="flex flex-1 items-center justify-center">
      <p className="text-base-content/40 text-sm">New document — not yet wired to SDK</p>
    </div>
  );
}

export const Route = createLazyFileRoute("/cabinet/editor/new")({
  component: NewEditor,
});
