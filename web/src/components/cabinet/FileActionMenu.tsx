import {
  ArrowBendUpRightIcon,
  DotsThreeVerticalIcon,
  DownloadSimpleIcon,
  EyeIcon,
  PencilSimpleIcon,
  ShareNetworkIcon,
  TrashIcon,
} from "@phosphor-icons/react";
import { DropdownMenu } from "@/components/DropdownMenu";
import { useAppStore } from "@/stores/app";
import type { FileItem } from "./types";

interface FileActionMenuProps {
  readonly item: FileItem;
  readonly onPreview?: () => void;
  readonly onEditMetadata?: () => void;
  readonly onRename?: () => void;
  readonly onMove?: () => void;
  readonly onShare?: () => void;
  readonly onDownload?: () => void;
  readonly onDelete?: () => void;
  readonly onDeleteFolder?: () => void;
}

export function FileActionMenu({
  item,
  onPreview,
  onEditMetadata,
  onRename,
  onMove,
  onShare,
  onDownload,
  onDelete,
  onDeleteFolder,
}: FileActionMenuProps) {
  const isFolder = item.kind === "folder";
  const downloading = useAppStore((s) => s.isLoading(`download:${item.uri}`));
  const deleting = useAppStore((s) => s.isLoading(`delete:${item.uri}`));

  if (!isFolder && (!item.decrypted || item.name === "[Keyring encrypted]")) return null;

  if (downloading || deleting) {
    return (
      <span
        className="loading loading-spinner loading-xs text-text-faint"
        role="status"
        aria-label={deleting ? `Deleting ${item.name}` : `Downloading ${item.name}`}
      />
    );
  }

  const isProposal = item.proposal != null;

  const items = isProposal
    ? [
        ...(onPreview ? [{ icon: EyeIcon, label: "Preview", onClick: onPreview }] : []),
        ...(!isFolder
          ? [{ icon: DownloadSimpleIcon, label: "Download", onClick: onDownload }]
          : []),
      ]
    : isFolder
      ? [
          { icon: PencilSimpleIcon, label: "Rename", onClick: onRename },
          { icon: ArrowBendUpRightIcon, label: "Move to\u2026", onClick: onMove },
          { icon: TrashIcon, label: "Delete", onClick: onDeleteFolder },
        ]
      : [
          ...(onPreview ? [{ icon: EyeIcon, label: "Preview", onClick: onPreview }] : []),
          { icon: PencilSimpleIcon, label: "Edit details", onClick: onEditMetadata },
          { icon: ShareNetworkIcon, label: "Share\u2026", onClick: onShare },
          { icon: ArrowBendUpRightIcon, label: "Move to\u2026", onClick: onMove },
          { icon: DownloadSimpleIcon, label: "Download", onClick: onDownload },
          { icon: TrashIcon, label: "Delete", onClick: onDelete },
        ];

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
