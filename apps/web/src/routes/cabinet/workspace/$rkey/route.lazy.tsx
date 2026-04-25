import { createLazyFileRoute, Outlet } from "@tanstack/react-router";

export const Route = createLazyFileRoute("/cabinet/workspace/$rkey")({
  component: () => <Outlet />,
});
