import { createFileRoute } from "@tanstack/react-router";
import { ogMeta } from "@/lib/og-meta";
import { MdxContent } from "@/components/content/MdxProvider";
import DocsIndexContent from "@/content/docs/index.mdx";

function DocsIndexPage() {
  return (
    <div className="px-6 pt-28 pb-20 sm:px-10">
      <MdxContent Content={DocsIndexContent} />
    </div>
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
