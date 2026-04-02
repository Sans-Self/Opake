import { forwardRef } from "react";
import { WarningIcon } from "@phosphor-icons/react";
import { ConfirmDialog, type ConfirmDialogHandle } from "@/components/ConfirmDialog";

interface DeleteConfirmDialogProps {
  readonly onConfirm: (uri: string) => void;
}

export const DeleteConfirmDialog = forwardRef<ConfirmDialogHandle, DeleteConfirmDialogProps>(
  function DeleteConfirmDialog({ onConfirm }, ref) {
    return (
      <ConfirmDialog
        ref={ref}
        title="Delete file?"
        icon={WarningIcon}
        iconClassName="text-error"
        iconBgClassName="bg-error/10"
        confirmLabel="Delete"
        confirmClassName="btn btn-error btn-sm rounded-lg text-xs"
        onConfirm={onConfirm}
      >
        {(fileName) => (
          <p>
            <span className="text-base-content font-medium">{fileName}</span> will be permanently
            deleted. This cannot be undone.
          </p>
        )}
      </ConfirmDialog>
    );
  },
);
