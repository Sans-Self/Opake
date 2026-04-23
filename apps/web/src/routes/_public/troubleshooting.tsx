import { createFileRoute } from "@tanstack/react-router";
import { ogMeta } from "@/lib/og-meta";
import { MdxContent } from "@/components/content/MdxProvider";
import TroubleshootingContent from "@/content/docs/use/troubleshooting.mdx";

function TroubleshootingPage() {
  return (
    <div className="mx-auto max-w-3xl px-6 pt-28 pb-20 sm:px-10">
      <MdxContent Content={TroubleshootingContent} className="prose" />
    </div>
  );
}

export const Route = createFileRoute("/_public/troubleshooting")({
  head: () => ({
    meta: ogMeta({
      title: "Troubleshooting — Opake",
      description: "Common issues and solutions for Opake.",
    }),
  }),
  component: TroubleshootingPage,
});
