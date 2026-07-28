import { createFileRoute } from "@tanstack/react-router";
import { ogMeta } from "@/lib/og-meta";
import { MdxContent } from "@/components/content/MdxProvider";
import { DocsLayout } from "@/components/content/docs-layout";
import DocsIndexContent from "@/content/docs/index.mdx";

function DocsIndexPage() {
  return (
    <DocsLayout>
      <MdxContent Content={DocsIndexContent} />
    </DocsLayout>
  );
}

export const Route = createFileRoute("/_public/docs/")({
  head: () => ({
    meta: ogMeta({
      title: "The Opaque Handbook — Opake",
      description: "Everything you need to get the most out of Opake.",
    }),
  }),
  component: DocsIndexPage,
});
