import { createFileRoute } from "@tanstack/react-router";

function SharedPage() {
  return (
    <div>
      <h1 className="text-xl font-semibold">Shared with me</h1>
      <p className="mt-2 text-neutral-500">Incoming shares go here.</p>
    </div>
  );
}

export const Route = createFileRoute("/shared")({
  component: SharedPage,
});
