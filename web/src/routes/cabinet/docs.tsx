import { createFileRoute } from "@tanstack/react-router";
import {
  SparkleIcon,
  LockIcon,
  ShareNetworkIcon,
  GraphIcon,
  QuestionIcon,
  ArrowSquareOutIcon,
} from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";

const DOCS_SECTIONS = [
  {
    id: "getting-started",
    title: "Getting Started",
    icon: SparkleIcon,
    desc: "Set up your cabinet, create your first encrypted file, and explore the interface.",
  },
  {
    id: "encryption",
    title: "Encryption & Keys",
    icon: LockIcon,
    desc: "How end-to-end encryption works in Opake and how your keys are managed.",
  },
  {
    id: "sharing",
    title: "Sharing & DIDs",
    icon: ShareNetworkIcon,
    desc: "Share files using decentralised identifiers without a central authority.",
  },
  {
    id: "at-protocol",
    title: "AT Protocol",
    icon: GraphIcon,
    desc: "The open standard powering Opake — identity, data portability, and federation.",
  },
  {
    id: "faq",
    title: "FAQ",
    icon: QuestionIcon,
    desc: "Common questions about privacy, security, and how Opake compares to alternatives.",
  },
];

function DocsPage() {
  const breadcrumbs = (
    <div className="breadcrumbs text-ui min-w-0 flex-1 overflow-hidden">
      <ul>
        <li>
          <span className="text-base-content font-medium">Docs & Help</span>
        </li>
      </ul>
    </div>
  );

  return (
    <PanelShell depth={1} breadcrumbs={breadcrumbs} footer="Documentation · Opake">
      <div className="p-5">
        <div className="mb-5">
          <div className="text-ui text-base-content mb-1 font-medium">Documentation</div>
          <div className="text-text-muted text-xs">
            Everything you need to get the most out of Opake.
          </div>
        </div>
        <div className="flex flex-col gap-2">
          {DOCS_SECTIONS.map((s) => (
            <div
              key={s.id}
              className="card card-bordered border-base-300/50 bg-base-100 cursor-pointer p-3.5"
            >
              <div className="bg-accent flex size-8 shrink-0 items-center justify-center rounded-lg">
                <s.icon size={14} className="text-primary" />
              </div>
              <div className="flex-1">
                <div className="text-ui text-base-content mb-0.5 font-medium">{s.title}</div>
                <div className="text-caption text-text-muted leading-relaxed">{s.desc}</div>
              </div>
              <ArrowSquareOutIcon size={12} className="text-text-faint mt-0.5 shrink-0" />
            </div>
          ))}
        </div>
      </div>
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/docs")({
  component: DocsPage,
});
