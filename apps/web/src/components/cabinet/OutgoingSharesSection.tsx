import { useCallback, useRef } from "react";
import { ProhibitIcon, ShareNetworkIcon } from "@phosphor-icons/react";
import { useAllShares, useRevokeShare } from "@opake/react";
import type { GrantEntry } from "@opake/sdk";
import { toastError, toastSuccess } from "@/stores/toast";
import { rkeyFromUri } from "@/lib/atUri";
import { formatShortDate } from "@/lib/format";
import { RevokeShareDialog } from "./RevokeShareDialog";
import type { ConfirmDialogHandle } from "@/components/ConfirmDialog";

export function OutgoingSharesSection() {
  const { data: shares, isLoading } = useAllShares();
  const revokeMut = useRevokeShare();
  const dialogRef = useRef<ConfirmDialogHandle>(null);

  const handleRevoke = useCallback(
    (grantUri: string) => {
      revokeMut.mutate(grantUri, {
        onSuccess: () => toastSuccess("Share revoked"),
        onError: (err) =>
          toastError(err instanceof Error ? err.message : "Failed to revoke share"),
      });
    },
    [revokeMut],
  );

  return (
    <section className="flex flex-col">
      <div className="border-base-300/40 flex items-center justify-between border-b px-3 py-2">
        <h2 className="text-text-muted text-[11px] font-semibold tracking-wide uppercase">
          Shared by you
        </h2>
        {shares && shares.length > 0 ? (
          <span className="text-text-faint text-[11px]">{shares.length}</span>
        ) : null}
      </div>

      {isLoading ? (
        <div className="flex justify-center py-8">
          <span className="loading loading-spinner loading-sm" />
        </div>
      ) : !shares || shares.length === 0 ? (
        <div className="flex flex-col items-center gap-1 py-6 text-center">
          <div className="bg-accent flex size-9 items-center justify-center rounded-lg">
            <ShareNetworkIcon size={16} className="text-text-faint" />
          </div>
          <div className="text-text-muted text-xs">You haven't shared anything yet</div>
        </div>
      ) : (
        <ul className="flex flex-col gap-px p-3">
          {shares.map((grant) => (
            <OutgoingRow key={grant.uri} grant={grant} onRevokeClick={() => dialogRef.current?.show(grant.uri, rkeyFromUri(grant.document))} />
          ))}
        </ul>
      )}

      <RevokeShareDialog ref={dialogRef} onConfirm={handleRevoke} />
    </section>
  );
}

interface OutgoingRowProps {
  readonly grant: GrantEntry;
  readonly onRevokeClick: () => void;
}

function OutgoingRow({ grant, onRevokeClick }: OutgoingRowProps) {
  const docLabel = rkeyFromUri(grant.document);

  return (
    <li className="border-base-300/40 hover:bg-base-200/40 flex items-center justify-between gap-3 rounded-lg border px-3 py-2 transition-colors">
      <div className="flex min-w-0 flex-1 items-center gap-3">
        <div className="bg-accent flex size-8 shrink-0 items-center justify-center rounded-lg">
          <ShareNetworkIcon size={16} className="text-primary" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-base-content truncate font-mono text-xs font-medium">{docLabel}</div>
          <div className="text-text-faint truncate text-[11px]">
            to {grant.recipient} · {formatShortDate(grant.createdAt)}
          </div>
        </div>
      </div>
      <button
        onClick={onRevokeClick}
        className="btn btn-ghost btn-xs gap-1.5 rounded-md"
        aria-label={`Revoke share of ${docLabel}`}
        title="Stop sharing"
      >
        <ProhibitIcon size={12} />
        <span className="text-[11px]">Revoke</span>
      </button>
    </li>
  );
}