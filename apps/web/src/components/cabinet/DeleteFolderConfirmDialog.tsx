import { forwardRef, useImperativeHandle, useRef, useState } from "react";
import { WarningIcon } from "@phosphor-icons/react";
import { ConfirmDialog, type ConfirmDialogHandle } from "@/components/ConfirmDialog";

export interface DeleteFolderDialogHandle {
  readonly show: (uri: string, name: string, documents: number, directories: number) => void;
}

interface DeleteFolderConfirmDialogProps {
  readonly onConfirm: (uri: string) => void;
}

export const DeleteFolderConfirmDialog = forwardRef<
  DeleteFolderDialogHandle,
  DeleteFolderConfirmDialogProps
>(function DeleteFolderConfirmDialog({ onConfirm }, ref) {
  const innerRef = useRef<ConfirmDialogHandle>(null);
  const [counts, setCounts] = useState({ documents: 0, directories: 0 });

  useImperativeHandle(ref, () => ({
    show: (uri: string, name: string, documents: number, directories: number) => {
      setCounts({ documents, directories });
      innerRef.current?.show(uri, name);
    },
  }));

  const contentsDescription = (() => {
    const parts = [
      counts.documents > 0 ? `${counts.documents} file${counts.documents === 1 ? "" : "s"}` : null,
      counts.directories > 0
        ? `${counts.directories} folder${counts.directories === 1 ? "" : "s"}`
        : null,
    ].filter((p): p is string => p !== null);
    return parts.length > 0 ? `Contains ${parts.join(" and ")}. ` : "";
  })();

  return (
    <ConfirmDialog
      ref={innerRef}
      title="Delete folder?"
      icon={WarningIcon}
      iconClassName="text-error"
      iconBgClassName="bg-error/10"
      confirmLabel="Delete"
      confirmClassName="btn btn-error btn-sm rounded-lg text-xs"
      onConfirm={onConfirm}
    >
      {(folderName) => (
        <p>
          <span className="text-base-content font-medium">{folderName}</span> and all its contents
          will be permanently deleted. {contentsDescription}This cannot be undone.
        </p>
      )}
    </ConfirmDialog>
  );
});
