import { DotsThreeVerticalIcon, DownloadSimpleIcon } from "@phosphor-icons/react";
import { DropdownMenu } from "@/components/DropdownMenu";
import type { FileItem } from "./types";

interface FileActionMenuProps {
  readonly item: FileItem;
  readonly downloading: boolean;
  readonly onDownload?: () => void;
}

export function FileActionMenu({ item, downloading, onDownload }: FileActionMenuProps) {
  const isFolder = item.kind === "folder";
  if (isFolder || !item.decrypted || item.name === "[Keyring encrypted]") return null;

  if (downloading) {
    return (
      <span
        className="loading loading-spinner loading-xs text-text-faint"
        role="status"
        aria-label={`Downloading ${item.name}`}
      />
    );
  }

  return (
    <DropdownMenu
      triggerClassName="btn btn-ghost btn-xs btn-square rounded-md"
      trigger={<DotsThreeVerticalIcon size={24} weight="bold" className="text-base-content" />}
      align="right"
      items={[{ icon: DownloadSimpleIcon, label: "Download", onClick: onDownload }]}
    />
  );
}
