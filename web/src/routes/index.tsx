import { createFileRoute } from "@tanstack/react-router";

function FilesPage() {
  return (
    <div>
      <h1 className="text-xl font-semibold">Files</h1>
      <p className="mt-2 text-neutral-500">File browser goes here.</p>
    </div>
  );
}

export const Route = createFileRoute("/")({
  component: FilesPage,
});
