import { createLazyFileRoute } from "@tanstack/react-router";

function Editor() {
  return (
    <div className="flex flex-1 items-center justify-center">
      <p className="text-base-content/40 text-sm">Editor — not yet wired to SDK</p>
    </div>
  );
}

export const Route = createLazyFileRoute("/cabinet/editor/$rkey")({
  component: Editor,
});
