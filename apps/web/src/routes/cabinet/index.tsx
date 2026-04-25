import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/cabinet/")({
  beforeLoad: () => {
    throw redirect({ to: "/cabinet/files" });
  },
});
