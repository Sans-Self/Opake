import {
  ArrowBendUpRightIcon,
  DotsThreeVerticalIcon,
  DownloadSimpleIcon,
  EyeIcon,
  NotePencilIcon,
  PencilSimpleIcon,
  ShareNetworkIcon,
  SlidersHorizontalIcon,
  TrashIcon,
} from "@phosphor-icons/react";
import { DropdownMenu } from "@/components/DropdownMenu";
import { useAppStore } from "@/stores/app";
import type { FileItem } from "./types";

interface FileActionMenuProps {
  readonly item: FileItem;
  readonly onPreview?: () => void;
  readonly onEdit?: () => void;
  readonly onEditMetadata?: () => void;
  readonly onRename?: () => void;
  readonly onMove?: () => void;
  readonly onShare?: () => void;
  readonly onManageSharing?: () => void;
  readonly onDownload?: () => void;
  readonly onDelete?: () => void;
  readonly onDeleteFolder?: () => void;
}

interface MenuItem {
  readonly icon: typeof EyeIcon;
  readonly label: string;
  readonly onClick?: () => void;
}

function buildFolderItems(props: FileActionMenuProps): readonly MenuItem[] {
  return [
    { icon: PencilSimpleIcon, label: "Rename", onClick: props.onRename },
    { icon: ArrowBendUpRightIcon, label: "Move to\u2026", onClick: props.onMove },
    { icon: TrashIcon, label: "Delete", onClick: props.onDeleteFolder },
  ];
}

function buildFileItems(props: FileActionMenuProps): readonly MenuItem[] {
  return [
    ...(props.onEdit ? [{ icon: NotePencilIcon, label: "Edit", onClick: props.onEdit }] : []),
    ...(props.onPreview ? [{ icon: EyeIcon, label: "Preview", onClick: props.onPreview }] : []),
    { icon: PencilSimpleIcon, label: "Edit details", onClick: props.onEditMetadata },
    // Share is gated by allowSharing upstream — the lexicon for workspace-
    // scoped sharing isn't in the protocol yet, so onShare is undefined in
    // workspace contexts and the menu entry is suppressed.
    ...(props.onShare
      ? [{ icon: ShareNetworkIcon, label: "Share\u2026", onClick: props.onShare }]
      : []),
    ...(props.onManageSharing
      ? [
          {
            icon: SlidersHorizontalIcon,
            label: "Manage sharing\u2026",
            onClick: props.onManageSharing,
          },
        ]
      : []),
    { icon: ArrowBendUpRightIcon, label: "Move to\u2026", onClick: props.onMove },
    { icon: DownloadSimpleIcon, label: "Download", onClick: props.onDownload },
    { icon: TrashIcon, label: "Delete", onClick: props.onDelete },
  ];
}

export function FileActionMenu(props: FileActionMenuProps) {
  const { item } = props;
  const downloading = useAppStore((s) => s.isLoading(`download:${item.uri}`));
  const deleting = useAppStore((s) => s.isLoading(`delete:${item.uri}`));

  // A provisional (pending) entry has no indexer-visible record behind it yet,
  // so it can never be an operation's target — no action menu, for folders and
  // files alike (indexer-consistency: provisional entries cannot be operated on).
  if (item.pending) {
    return null;
  }

  if (item.kind !== "folder" && (!item.decrypted || item.name === "[Keyring encrypted]")) {
    return null;
  }

  if (downloading || deleting) {
    return (
      <span
        className="loading loading-spinner loading-xs text-text-faint"
        role="status"
        aria-label={deleting ? `Deleting ${item.name}` : `Downloading ${item.name}`}
      />
    );
  }

  const items = item.kind === "folder" ? buildFolderItems(props) : buildFileItems(props);

  return (
    // eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions -- stopPropagation wrapper to prevent folder row navigation
    <div onClick={(e) => e.stopPropagation()}>
      <DropdownMenu
        triggerClassName="btn btn-ghost btn-xs btn-square rounded-md"
        trigger={<DotsThreeVerticalIcon size={24} weight="bold" className="text-base-content" />}
        align="right"
        items={items}
      />
    </div>
  );
}
