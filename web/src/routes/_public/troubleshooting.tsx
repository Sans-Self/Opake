import { createFileRoute } from "@tanstack/react-router";
import { MdxContent } from "@/components/content/MdxProvider";
import TroubleshootingContent from "@/content/troubleshooting.mdx";

function TroubleshootingPage() {
  return (
    <div className="mx-auto max-w-3xl px-6 pt-28 pb-20 sm:px-10">
      <MdxContent Content={TroubleshootingContent} className="prose" />
    </div>
  );
}

export const Route = createFileRoute("/_public/troubleshooting")({
  head: () => ({
    meta: [
      { title: "Troubleshooting — Opake" },
      {
        name: "description",
        content: "Common issues and solutions for Opake.",
      },
    ],
  }),
  component: TroubleshootingPage,
});
