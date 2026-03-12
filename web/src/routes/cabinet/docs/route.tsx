import { createFileRoute, Outlet } from "@tanstack/react-router";

function DocsLayout() {
  return <Outlet />;
}

export const Route = createFileRoute("/cabinet/docs")({
  component: DocsLayout,
});
