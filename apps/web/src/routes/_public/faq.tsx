import { createFileRoute } from "@tanstack/react-router";
import { ogMeta } from "@/lib/og-meta";
import { MdxContent } from "@/components/content/MdxProvider";
import FaqContent from "@/content/docs/faq.mdx";

function FaqPage() {
  return (
    <div className="px-6 pt-16 pb-20 sm:px-10">
      <MdxContent Content={FaqContent} />
    </div>
  );
}

export const Route = createFileRoute("/_public/faq")({
  head: () => ({
    meta: ogMeta({
      title: "FAQ — Opake",
      description: "Frequently asked questions about Opake.",
      image: "/og/faq.png",
    }),
  }),
  component: FaqPage,
});
