import {
  SparkleIcon,
  LockIcon,
  ShareNetworkIcon,
  GraphIcon,
  QuestionIcon,
  ArrowSquareOutIcon,
  UserIcon,
  ShieldCheckIcon,
  BellIcon,
  CaretRightIcon,
  TrashIcon,
  FolderIcon,
} from "@phosphor-icons/react"
import { FileListRow } from "./FileListRow"
import { FileGridCard } from "./FileGridCard"
import type { FileItem, Panel } from "./types"
import { ROOT_ITEMS, SHARED_ITEMS, DOCUMENTS_ITEMS } from "./mock-data"

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
]

const SETTINGS_SECTIONS = [
  { label: "Account & Identity", desc: "DID: did:plc:7f2ab3c4d…8e91f0", icon: UserIcon },
  { label: "Encryption Keys", desc: "Last rotated 14 days ago · Active", icon: LockIcon },
  { label: "Sharing & Permissions", desc: "3 active collaborators", icon: ShareNetworkIcon },
  { label: "Connected Devices", desc: "2 devices linked", icon: ShieldCheckIcon },
  { label: "Notifications", desc: "Email & in-app alerts", icon: BellIcon },
]

const ALL_ITEMS = [...ROOT_ITEMS, ...SHARED_ITEMS, ...DOCUMENTS_ITEMS]

function getItemsForPanel(panel: Panel, starredIds: ReadonlySet<string>): FileItem[] {
  const baseItems = (() => {
    switch (panel.type) {
      case "root":
        return ROOT_ITEMS
      case "shared":
        return SHARED_ITEMS
      case "starred":
        return ALL_ITEMS.filter((i) => starredIds.has(i.id))
      case "encrypted":
        return ROOT_ITEMS.filter((i) => i.status === "private")
      case "folder":
        return panel.folderId === "f-documents" ? DOCUMENTS_ITEMS : ROOT_ITEMS.slice(5)
      default:
        return []
    }
  })()

  return baseItems.map((item) => ({
    ...item,
    starred: starredIds.has(item.id),
  }))
}

interface PanelContentProps {
  panel: Panel
  viewMode: "list" | "grid"
  starredIds: ReadonlySet<string>
  onOpen: (item: FileItem) => void
  onStar: (id: string) => void
}

export function PanelContent({
  panel,
  viewMode,
  starredIds,
  onOpen,
  onStar,
}: Readonly<PanelContentProps>) {
  // Docs
  if (panel.type === "docs") {
    return (
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
    )
  }

  // Settings
  if (panel.type === "settings") {
    return (
      <div className="p-5">
        <div className="mb-5">
          <div className="text-ui text-base-content mb-1 font-medium">Settings</div>
          <div className="text-text-muted text-xs">Manage your account, keys, and preferences.</div>
        </div>
        <div className="divider mt-0 mb-4" />
        <div className="flex flex-col gap-1.5">
          {SETTINGS_SECTIONS.map(({ label, desc, icon: Icon }) => (
            <div
              key={label}
              className="card card-bordered border-base-300/50 bg-base-100 cursor-pointer p-3.5"
            >
              <div className="bg-bg-stone flex size-8 shrink-0 items-center justify-center rounded-lg">
                <Icon size={14} className="text-text-muted" />
              </div>
              <div className="flex-1">
                <div className="text-ui text-base-content font-medium">{label}</div>
                <div className="text-caption text-text-muted">{desc}</div>
              </div>
              <CaretRightIcon size={13} className="text-text-faint" />
            </div>
          ))}
        </div>
      </div>
    )
  }

  // TrashIcon
  if (panel.type === "trash") {
    return (
      <div className="hero py-16">
        <div className="hero-content flex-col text-center">
          <div className="bg-bg-stone flex size-13 items-center justify-center rounded-[14px]">
            <TrashIcon size={22} className="text-text-faint" />
          </div>
          <div className="text-ui text-text-muted">TrashIcon is empty</div>
          <div className="text-text-faint max-w-60 text-xs leading-relaxed">
            Deleted files appear here for 30 days before permanent removal.
          </div>
        </div>
      </div>
    )
  }

  // FileIcon browser (list / grid)
  const items = getItemsForPanel(panel, starredIds)

  if (items.length === 0) {
    return (
      <div className="hero py-16">
        <div className="hero-content flex-col text-center">
          <div className="bg-accent flex size-13 items-center justify-center rounded-[14px]">
            <FolderIcon size={22} className="text-text-faint" />
          </div>
          <div className="text-ui text-text-muted">Nothing here yet</div>
        </div>
      </div>
    )
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
  )
}
