import {
  FolderIcon,
  FileTextIcon,
  FileIcon,
  FileImageIcon,
  MagnifyingGlassIcon,
  BellIcon,
  LockIcon,
  UsersIcon,
  BookOpenIcon,
  GearIcon,
  ShieldCheckIcon,
  CaretRightIcon,
} from "@phosphor-icons/react";
import type { Icon as PhosphorIcon } from "@phosphor-icons/react";
import { OpakeLogo } from "./OpakeLogo";

// ─── Static data ──────────────────────────────────────────────────────────────

interface NavEntry {
  readonly icon: PhosphorIcon;
  readonly label: string;
  readonly active?: boolean;
}

interface WorkspaceEntry {
  readonly name: string;
  readonly count: number;
}

interface MockFile {
  readonly name: string;
  readonly meta: string;
  readonly folder?: boolean;
  readonly iconBg: string;
  readonly iconText: string;
  readonly icon: PhosphorIcon;
  readonly badge?: { label: string; className: string };
}

const MAIN_NAV: readonly NavEntry[] = [
  { icon: FolderIcon, label: "Your Cabinet", active: true },
  { icon: UsersIcon, label: "Sharing" },
];

const BOTTOM_NAV: readonly NavEntry[] = [
  { icon: BookOpenIcon, label: "Docs & Help" },
  { icon: GearIcon, label: "Settings" },
];

const WORKSPACES: readonly WorkspaceEntry[] = [
  { name: "Family Photos", count: 23 },
  { name: "Work Projects", count: 12 },
];

const FILES: readonly MockFile[] = [
  {
    name: "Documents",
    meta: "23 items",
    folder: true,
    iconBg: "bg-accent",
    iconText: "text-primary",
    icon: FolderIcon,
    badge: { label: "Private", className: "badge-accent text-primary border-border-accent" },
  },
  {
    name: "Projects",
    meta: "Shared · 7 items",
    folder: true,
    iconBg: "bg-accent",
    iconText: "text-primary",
    icon: FolderIcon,
    badge: { label: "Shared", className: "bg-bg-sage text-success border-success/30" },
  },
  {
    name: "Q4 Strategy.docx",
    meta: "245 KB · 2 days ago",
    iconBg: "bg-file-doc-bg",
    iconText: "text-file-doc",
    icon: FileTextIcon,
    badge: { label: "Private", className: "badge-accent text-primary border-border-accent" },
  },
  {
    name: "Budget 2026.xlsx",
    meta: "1.2 MB · Shared",
    iconBg: "bg-file-sheet-bg",
    iconText: "text-file-sheet",
    icon: FileIcon,
    badge: { label: "Shared", className: "bg-bg-sage text-success border-success/30" },
  },
  {
    name: "Design Brief.pdf",
    meta: "3.4 MB · Yesterday",
    iconBg: "bg-file-pdf-bg",
    iconText: "text-file-pdf",
    icon: FileTextIcon,
    badge: { label: "Private", className: "badge-accent text-primary border-border-accent" },
  },
  {
    name: "architecture.png",
    meta: "890 KB · 3 days ago",
    iconBg: "bg-file-image-bg",
    iconText: "text-file-image",
    icon: FileImageIcon,
    badge: { label: "Private", className: "badge-accent text-primary border-border-accent" },
  },
];

// ─── Subcomponents ────────────────────────────────────────────────────────────

function MockSidebar() {
  return (
    <aside className="border-base-300/50 bg-base-200 flex w-48 shrink-0 flex-col border-r px-3 py-4">
      <div className="mb-5 px-0.5">
        <OpakeLogo />
      </div>

      <nav className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto">
        {MAIN_NAV.map(({ icon: Icon, label, active }) => (
          <div
            key={label}
            className={`text-ui flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.75 ${
              active ? "bg-accent text-primary" : "text-text-muted"
            }`}
          >
            <Icon size={14} weight={active ? "fill" : "regular"} />
            <span className="flex-1">{label}</span>
          </div>
        ))}

        <div className="text-label text-text-faint mt-3.5 mb-1.5 ml-1 tracking-widest uppercase">
          Workspaces
        </div>
        {WORKSPACES.map(({ name, count }) => (
          <div
            key={name}
            className="text-ui text-text-muted flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.75"
          >
            <div className="bg-accent text-micro text-primary flex size-5 shrink-0 items-center justify-center rounded-md font-semibold">
              {name[0]}
            </div>
            <span className="flex-1">{name}</span>
            <span className="text-label text-text-faint">{count}</span>
          </div>
        ))}
      </nav>

      <div className="divider mx-1 my-0" />
      <div className="flex flex-col gap-0.5">
        {BOTTOM_NAV.map(({ icon: Icon, label }) => (
          <div
            key={label}
            className="text-ui text-text-muted flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.75"
          >
            <Icon size={14} />
            <span className="flex-1">{label}</span>
          </div>
        ))}
      </div>
    </aside>
  );
}

function MockTopBar() {
  return (
    <header className="border-base-300/50 bg-base-300/90 flex shrink-0 items-center gap-3 border-b px-5 py-2.5 backdrop-blur-[10px]">
      <div className="input input-bordered border-base-300/50 bg-base-100/80 text-ui flex max-w-72 flex-1 items-center gap-2 rounded-lg py-1.75">
        <MagnifyingGlassIcon size={13} className="text-text-faint" />
        <span className="text-text-faint text-ui">Search your cabinet…</span>
      </div>

      <div className="flex-1" />

      <div className="btn btn-ghost btn-sm btn-square rounded-lg">
        <BellIcon size={15} className="text-text-muted" />
      </div>

      <div className="btn btn-ghost btn-sm gap-2 rounded-lg pl-1">
        <div className="bg-accent text-caption text-primary flex size-7 items-center justify-center rounded-full font-semibold">
          V
        </div>
        <span className="text-secondary text-xs font-normal">veerle.bsky.social</span>
      </div>
    </header>
  );
}

function MockFileRow({ file }: Readonly<{ file: MockFile }>) {
  const Icon = file.icon;
  return (
    <div className="hover:bg-bg-hover flex items-center gap-3 rounded-xl px-3 py-2.25 transition-colors">
      <div
        className={`flex size-8 shrink-0 items-center justify-center rounded-lg ${file.iconBg} ${file.iconText}`}
      >
        <Icon size={15} weight={file.folder ? "fill" : "regular"} />
      </div>

      <div className="min-w-0 flex-1">
        <div className="text-ui text-base-content flex items-center truncate">
          {file.name}
          {file.folder && (
            <>
              &nbsp;&nbsp;
              <CaretRightIcon size={13} className="text-text-faint" />
            </>
          )}
        </div>
        <div className="text-caption text-text-faint mt-0.5">{file.meta}</div>
      </div>

      {file.badge && (
        <span
          className={`badge badge-sm text-label gap-1 border tracking-wide ${file.badge.className}`}
        >
          <LockIcon size={8} weight="bold" />
          {file.badge.label}
        </span>
      )}
    </div>
  );
}

function MockPanelShell() {
  return (
    <div className="relative flex-1 overflow-hidden p-5.5 pl-7">
      {/* Ghost panels */}
      <div className="border-primary/15 bg-bg-ghost-1 absolute inset-y-5.5 right-5.5 left-7 z-1 -translate-x-2.5 -translate-y-2.5 rounded-2xl border" />
      <div className="border-base-300/50 bg-bg-ghost-2 shadow-panel-sm absolute inset-y-5.5 right-5.5 left-7 z-2 -translate-x-1.25 -translate-y-1.25 rounded-2xl border" />

      {/* Active panel */}
      <div className="border-base-300/50 bg-base-100 shadow-panel-lg absolute inset-y-5.5 right-5.5 left-7 z-10 flex flex-col overflow-hidden rounded-2xl border">
        {/* Header */}
        <div className="border-base-300/50 bg-base-100/70 flex shrink-0 items-center gap-2.5 border-b px-4 py-2.75">
          <div className="text-ui flex flex-1 items-center gap-1.5">
            <span className="text-text-faint">Cabinet</span>
            <span className="text-text-faint text-label">›</span>
            <span className="text-base-content font-medium">Documents</span>
          </div>
        </div>

        {/* File list */}
        <div className="min-h-0 flex-1 overflow-y-auto p-3">
          <div className="flex flex-col gap-px">
            {FILES.map((file) => (
              <MockFileRow key={file.name} file={file} />
            ))}
          </div>
        </div>

        {/* Footer */}
        <div className="border-base-300/50 bg-base-100/60 flex shrink-0 items-center gap-2 border-t px-4 py-2.25">
          <ShieldCheckIcon size={11} className="text-primary" />
          <span className="text-caption text-text-faint">End-to-end encrypted</span>
          <div className="flex-1" />
          <span className="font-display text-ui text-text-faint italic">3 panels deep</span>
        </div>
      </div>
    </div>
  );
}

// ─── Exported mockup ──────────────────────────────────────────────────────────

export function CabinetMockup() {
  return (
    <div className="shadow-panel-lg overflow-hidden rounded-2xl border border-[rgba(112,83,40,0.13)]">
      {/* Browser chrome */}
      <div className="bg-base-100 flex items-center gap-1.5 border-b border-[rgba(112,83,40,0.13)] px-4 py-2.5">
        <div className="size-2.5 rounded-full bg-[#D9B8A0]" />
        <div className="size-2.5 rounded-full bg-[#D4C4A8]" />
        <div className="size-2.5 rounded-full bg-[#C8D4B8]" />
        <div className="flex flex-1 justify-center">
          <div className="bg-base-300 flex h-5.5 w-48 items-center gap-1.5 rounded-md border border-[rgba(112,83,40,0.13)] px-3">
            <LockIcon size={9} className="text-text-faint" />
            <span className="text-text-faint text-[10px]">opake.app/cabinet</span>
          </div>
        </div>
      </div>

      {/* App layout */}
      <div className="bg-base-300 flex h-96">
        <MockSidebar />
        <div className="flex flex-1 flex-col overflow-hidden">
          <MockTopBar />
          <MockPanelShell />
        </div>
      </div>
    </div>
  );
}
