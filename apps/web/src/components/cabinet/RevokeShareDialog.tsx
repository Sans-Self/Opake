import { forwardRef } from "react";
import { ProhibitIcon } from "@phosphor-icons/react";
import { ConfirmDialog, type ConfirmDialogHandle } from "@/components/ConfirmDialog";

interface RevokeShareDialogProps {
  readonly onConfirm: (grantUri: string) => void;
}

export const RevokeShareDialog = forwardRef<ConfirmDialogHandle, RevokeShareDialogProps>(
  function RevokeShareDialog({ onConfirm }, ref) {
    return (
      <ConfirmDialog
        ref={ref}
        title="Stop sharing?"
        icon={ProhibitIcon}
        iconClassName="text-warning"
        iconBgClassName="bg-warning/10"
        confirmLabel="Stop sharing"
        confirmClassName="btn btn-warning btn-sm rounded-lg text-xs"
        onConfirm={onConfirm}
      >
        {(fileName) => (
          <p>
            <span className="text-base-content font-medium">{fileName}</span> will no longer be
            accessible to the recipient.
          </p>
        )}
      </ConfirmDialog>
    );
  },
);
