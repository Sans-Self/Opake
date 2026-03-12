import { createFileRoute } from "@tanstack/react-router";
import { MdxContent } from "@/components/content/MdxProvider";
import FaqContent from "@/content/faq.mdx";

function FaqPage() {
  return (
    <div className="px-6 pt-16 pb-20 sm:px-10">
      <MdxContent Content={FaqContent} />
    </div>
  );
}

export const Route = createFileRoute("/_public/faq")({
  head: () => ({
    meta: [
      { title: "FAQ — Opake" },
      { name: "description", content: "Frequently asked questions about Opake." },
    ],
  }),
  component: FaqPage,
});
