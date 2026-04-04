import { createLazyFileRoute } from "@tanstack/react-router";

function FilesPath() {
  return (
    <div className="flex flex-1 items-center justify-center">
      <p className="text-base-content/40 text-sm">Directory view — not yet wired to SDK</p>
    </div>
  );
}

export const Route = createLazyFileRoute("/cabinet/files/$")({
  component: FilesPath,
});
