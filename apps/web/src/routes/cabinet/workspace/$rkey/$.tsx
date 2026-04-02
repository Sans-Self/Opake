import { createFileRoute } from "@tanstack/react-router";

// Splat route for workspace document preview URLs.
// The parent ($rkey.tsx) reads the splat param via useMatch to determine
// which document to preview. This route renders nothing — the parent
// handles the split-panel layout.
function WorkspacePreviewSplat() {
  return null;
}

export const Route = createFileRoute("/cabinet/workspace/$rkey/$")({
  ssr: false,
  component: WorkspacePreviewSplat,
});
