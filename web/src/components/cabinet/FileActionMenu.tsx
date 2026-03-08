import { DotsThreeVerticalIcon, DownloadSimpleIcon, TrashIcon } from "@phosphor-icons/react";
import { DropdownMenu } from "@/components/DropdownMenu";
import { useAppStore } from "@/stores/app";
import type { FileItem } from "./types";

interface FileActionMenuProps {
  readonly item: FileItem;
  readonly onDownload?: () => void;
  readonly onDelete?: () => void;
}

export function FileActionMenu({ item, onDownload, onDelete }: FileActionMenuProps) {
  const isFolder = item.kind === "folder";
  const downloading = useAppStore((s) => s.isLoading(`download:${item.uri}`));
  const deleting = useAppStore((s) => s.isLoading(`delete:${item.uri}`));

  if (isFolder || !item.decrypted || item.name === "[Keyring encrypted]") return null;

  if (downloading || deleting) {
    return (
      <span
        className="loading loading-spinner loading-xs text-text-faint"
        role="status"
        aria-label={deleting ? `Deleting ${item.name}` : `Downloading ${item.name}`}
      />
    );
  }

  return (
    <DropdownMenu
      triggerClassName="btn btn-ghost btn-xs btn-square rounded-md"
      trigger={<DotsThreeVerticalIcon size={24} weight="bold" className="text-base-content" />}
      align="right"
      items={[
        { icon: DownloadSimpleIcon, label: "Download", onClick: onDownload },
        { icon: TrashIcon, label: "Delete", onClick: onDelete },
      ]}
    />
  );
}
