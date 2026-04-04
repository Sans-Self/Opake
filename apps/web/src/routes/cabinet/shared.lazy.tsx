import { createLazyFileRoute } from "@tanstack/react-router";

export const Route = createLazyFileRoute("/cabinet/shared")({
  component: RouteComponent,
});

function RouteComponent() {
  return <div>Hello "/cabinet/shared"!</div>;
}
