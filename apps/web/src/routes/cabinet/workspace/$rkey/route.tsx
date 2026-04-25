import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/cabinet/workspace/$rkey")({
  ssr: false,
});
