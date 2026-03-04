import {
  Sparkle,
  Lock,
  ShareNetwork,
  Graph,
  Question,
  ArrowSquareOut,
  User,
  ShieldCheck,
  Bell,
  CaretRight,
  Trash,
  Folder,
} from "@phosphor-icons/react";
import { FileListRow } from "./FileListRow";
import { FileGridCard } from "./FileGridCard";
import type { FileItem, Panel } from "./types";
import {
  ROOT_ITEMS,
  SHARED_ITEMS,
  STARRED_ITEMS,
  DOCUMENTS_ITEMS,
} from "./mock-data";

const DOCS_SECTIONS = [
  { id: "getting-started", title: "Getting Started", icon: Sparkle, desc: "Set up your cabinet, create your first encrypted file, and explore the interface." },
  { id: "encryption", title: "Encryption & Keys", icon: Lock, desc: "How end-to-end encryption works in Opake and how your keys are managed." },
  { id: "sharing", title: "Sharing & DIDs", icon: ShareNetwork, desc: "Share files using decentralised identifiers without a central authority." },
  { id: "at-protocol", title: "AT Protocol", icon: Graph, desc: "The open standard powering Opake — identity, data portability, and federation." },
  { id: "faq", title: "FAQ", icon: Question, desc: "Common questions about privacy, security, and how Opake compares to alternatives." },
];

const SETTINGS_SECTIONS = [
  { label: "Account & Identity", desc: "DID: did:plc:7f2ab3c4d…8e91f0", icon: User },
  { label: "Encryption Keys", desc: "Last rotated 14 days ago · Active", icon: Lock },
  { label: "Sharing & Permissions", desc: "3 active collaborators", icon: ShareNetwork },
  { label: "Connected Devices", desc: "2 devices linked", icon: ShieldCheck },
  { label: "Notifications", desc: "Email & in-app alerts", icon: Bell },
];

function getItemsForPanel(panel: Panel): FileItem[] {
  switch (panel.type) {
    case "root":
      return ROOT_ITEMS;
    case "shared":
      return SHARED_ITEMS;
    case "starred":
      return STARRED_ITEMS;
    case "encrypted":
      return ROOT_ITEMS.filter((i) => i.status === "private");
    case "folder":
      return panel.data?.id === "f-documents"
        ? DOCUMENTS_ITEMS
        : ROOT_ITEMS.slice(5);
    default:
      return [];
  }
}

interface PanelContentProps {
  panel: Panel;
  viewMode: "list" | "grid";
  onOpen: (item: FileItem) => void;
  onStar: (id: string) => void;
}

export function PanelContent({
  panel,
  viewMode,
  onOpen,
  onStar,
}: PanelContentProps) {
  // Docs
  if (panel.type === "docs") {
    return (
      <div className="p-5">
        <div className="mb-5">
          <div className="mb-1 text-[13px] font-medium text-base-content">
            Documentation
          </div>
          <div className="text-[12px] text-text-muted">
            Everything you need to get the most out of Opake.
          </div>
        </div>
        <div className="flex flex-col gap-2">
          {DOCS_SECTIONS.map((s) => (
            <div
              key={s.id}
              className="flex cursor-pointer items-start gap-3 rounded-[10px] border border-base-300/50 bg-base-100 p-3.5"
            >
              <div className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-accent">
                <s.icon size={14} className="text-primary" />
              </div>
              <div className="flex-1">
                <div className="mb-0.5 text-[13px] font-medium text-base-content">
                  {s.title}
                </div>
                <div className="text-[11px] leading-relaxed text-text-muted">
                  {s.desc}
                </div>
              </div>
              <ArrowSquareOut
                size={12}
                className="mt-0.5 shrink-0 text-text-faint"
              />
            </div>
          ))}
        </div>
      </div>
    );
  }

  // Settings
  if (panel.type === "settings") {
    return (
      <div className="p-5">
        <div className="mb-5">
          <div className="mb-1 text-[13px] font-medium text-base-content">
            Settings
          </div>
          <div className="text-[12px] text-text-muted">
            Manage your account, keys, and preferences.
          </div>
        </div>
        <div className="divider mt-0 mb-4" />
        <div className="flex flex-col gap-1.5">
          {SETTINGS_SECTIONS.map(({ label, desc, icon: Icon }) => (
            <div
              key={label}
              className="flex cursor-pointer items-center gap-3 rounded-[10px] border border-base-300/50 bg-base-100 p-3.5"
            >
              <div className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-bg-stone">
                <Icon size={14} className="text-text-muted" />
              </div>
              <div className="flex-1">
                <div className="text-[13px] font-medium text-base-content">
                  {label}
                </div>
                <div className="text-[11px] text-text-muted">{desc}</div>
              </div>
              <CaretRight size={13} className="text-text-faint" />
            </div>
          ))}
        </div>
      </div>
    );
  }

  // Trash
  if (panel.type === "trash") {
    return (
      <div className="hero py-16">
        <div className="hero-content flex-col text-center">
          <div className="flex size-[52px] items-center justify-center rounded-[14px] bg-bg-stone">
            <Trash size={22} className="text-text-faint" />
          </div>
          <div className="text-[13px] text-text-muted">Trash is empty</div>
          <div className="max-w-[240px] text-[12px] leading-relaxed text-text-faint">
            Deleted files appear here for 30 days before permanent removal.
          </div>
        </div>
      </div>
    );
  }

  // File browser (list / grid)
  const items = getItemsForPanel(panel);

  if (items.length === 0) {
    return (
      <div className="hero py-16">
        <div className="hero-content flex-col text-center">
          <div className="flex size-[52px] items-center justify-center rounded-[14px] bg-accent">
            <Folder size={22} className="text-text-faint" />
          </div>
          <div className="text-[13px] text-text-muted">Nothing here yet</div>
        </div>
      </div>
    );
  }

  return (
    <div className="p-3">
      {viewMode === "list" ? (
        <div className="flex flex-col gap-px">
          {items.map((item) => (
            <FileListRow
              key={item.id}
              item={item}
              onClick={() => item.kind === "folder" && onOpen(item)}
              onStar={() => onStar(item.id)}
            />
          ))}
        </div>
      ) : (
        <div className="grid grid-cols-2 gap-3">
          {items.map((item) => (
            <FileGridCard
              key={item.id}
              item={item}
              onClick={() => item.kind === "folder" && onOpen(item)}
            />
          ))}
        </div>
      )}
    </div>
  );
}
