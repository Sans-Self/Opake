import { createLazyFileRoute, Outlet } from "@tanstack/react-router";

function DocsLayout() {
  return <Outlet />;
}

export const Route = createLazyFileRoute("/cabinet/docs")({
  component: DocsLayout,
});
