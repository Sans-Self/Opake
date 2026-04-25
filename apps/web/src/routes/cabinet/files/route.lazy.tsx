import { createLazyFileRoute, Outlet } from "@tanstack/react-router";

export const Route = createLazyFileRoute("/cabinet/files")({
  component: () => <Outlet />,
});
