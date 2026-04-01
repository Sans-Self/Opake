import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import { LinkIcon, CopyIcon, TrashIcon, CheckIcon, ShareNetworkIcon } from "@phosphor-icons/react";
import { MODAL_TRANSITION_MS } from "@/components/ConfirmDialog";
import { getOpakeWorker } from "@/lib/worker";
import { toastSuccess, toastError } from "@/stores/toast";
import type { InvitationEntry } from "@/workers/api/workspace";
import type { WorkspaceRole } from "@/lib/workspaceSchemas";

export interface InviteDialogHandle {
  readonly show: (keyringUri: string) => void;
}

const ROLE_OPTIONS: readonly { readonly value: WorkspaceRole; readonly label: string }[] = [
  { value: "editor", label: "Editor" },
  { value: "viewer", label: "Viewer" },
  { value: "manager", label: "Manager" },
];

function buildInviteLink(inviterDid: string, token: string): string {
  return `${window.location.origin}/invite?from=${encodeURIComponent(inviterDid)}&token=${encodeURIComponent(token)}`;
}

export const InviteDialog = forwardRef<InviteDialogHandle, object>(
  function InviteDialog(_props, ref) {
    const dialogRef = useRef<HTMLDialogElement>(null);
    const [keyringUri, setKeyringUri] = useState<string | null>(null);
    const [role, setRole] = useState<WorkspaceRole>("editor");
    const [invitations, setInvitations] = useState<readonly InvitationEntry[]>([]);
    const [loading, setLoading] = useState(false);
    const [copiedUri, setCopiedUri] = useState<string | null>(null);

    const dismiss = useCallback(() => {
      dialogRef.current?.close();
      setTimeout(() => {
        setKeyringUri(null);
        setInvitations([]);
        setCopiedUri(null);
      }, MODAL_TRANSITION_MS);
    }, []);

    const loadInvitations = useCallback(async () => {
      setLoading(true);
      try {
        const worker = getOpakeWorker();
        const all = await worker.listInvitations();
        // Filter to invitations for this workspace
        setInvitations(keyringUri ? all.filter((inv) => inv.target === keyringUri) : []);
      } catch {
        toastError("Failed to load invitations");
      } finally {
        setLoading(false);
      }
    }, [keyringUri]);

    useEffect(() => {
      if (keyringUri) void loadInvitations();
    }, [keyringUri, loadInvitations]);

    const handleCreate = useCallback(async () => {
      if (!keyringUri) return;
      try {
        const worker = getOpakeWorker();
        await worker.createInvitation(keyringUri, role);
        toastSuccess("Invitation created");
        await loadInvitations();
      } catch (err) {
        toastError(
          `Failed to create invitation: ${err instanceof Error ? err.message : String(err)}`,
        );
      }
    }, [keyringUri, role, loadInvitations]);

    const handleRevoke = useCallback(
      async (invitationUri: string) => {
        try {
          const worker = getOpakeWorker();
          await worker.revokeInvitation(invitationUri);
          toastSuccess("Invitation revoked");
          await loadInvitations();
        } catch (err) {
          toastError(`Failed to revoke: ${err instanceof Error ? err.message : String(err)}`);
        }
      },
      [loadInvitations],
    );

    const handleCopy = useCallback(async (inv: InvitationEntry) => {
      // [NOI FEEDBACK PLS] — what should the invite link format be?
      // Currently: /invite?from=<inviterDid>&token=<token>
      // The inviter DID is in the invitation URI authority.
      const inviterDid = inv.uri.split("/")[2];
      const link = buildInviteLink(inviterDid, inv.token);
      await navigator.clipboard.writeText(link);
      setCopiedUri(inv.uri);
      setTimeout(() => setCopiedUri(null), 2000);
    }, []);

    useImperativeHandle(ref, () => ({
      show: (uri: string) => {
        setKeyringUri(uri);
        setRole("editor");
        setCopiedUri(null);
        dialogRef.current?.showModal();
      },
    }));

    return (
      <dialog ref={dialogRef} className="modal" aria-label="Invite to workspace">
        <div className="modal-box max-w-sm">
          <div className="flex flex-col items-center gap-3 text-center">
            <div className="bg-accent/60 flex size-11 items-center justify-center rounded-full">
              <ShareNetworkIcon size={20} className="text-primary" />
            </div>
            <h3 className="text-base-content text-sm font-semibold">Invite link</h3>
          </div>

          {/* Create new invitation */}
          <div className="mt-4 flex items-center gap-2">
            <div className="flex flex-1 gap-1.5" role="radiogroup" aria-label="Invitation role">
              {ROLE_OPTIONS.map((option) => (
                <button
                  key={option.value}
                  type="button"
                  role="radio"
                  aria-checked={role === option.value}
                  onClick={() => setRole(option.value)}
                  className={`btn btn-xs flex-1 rounded-lg ${
                    role === option.value ? "btn-primary" : "btn-ghost"
                  }`}
                >
                  {option.label}
                </button>
              ))}
            </div>
            <button
              onClick={() => void handleCreate()}
              className="btn btn-primary btn-xs gap-1 rounded-lg"
            >
              <LinkIcon size={12} />
              Create
            </button>
          </div>

          {/* Existing invitations */}
          <div className="mt-3 flex flex-col gap-1">
            {loading && (
              <span className="text-caption text-text-faint py-2 text-center">Loading…</span>
            )}
            {!loading && invitations.length === 0 && (
              <span className="text-caption text-text-faint py-2 text-center">
                No active invitations
              </span>
            )}
            {invitations.map((inv) => {
              const isCopied = copiedUri === inv.uri;
              return (
                <div key={inv.uri} className="flex items-center gap-2 rounded-lg px-2.5 py-1.5">
                  <LinkIcon size={13} className="text-text-muted shrink-0" />
                  <div className="flex min-w-0 flex-1 flex-col">
                    <span className="text-ui text-base-content truncate">
                      {inv.role ?? "viewer"} invite
                    </span>
                    <span className="text-caption text-text-faint">
                      {inv.uses} uses
                      {inv.maxUses != null && ` / ${inv.maxUses} max`}
                    </span>
                  </div>
                  <button
                    onClick={() => void handleCopy(inv)}
                    className="btn btn-ghost btn-xs rounded-lg"
                    title="Copy invite link"
                    aria-label="Copy invite link"
                  >
                    {isCopied ? <CheckIcon size={13} /> : <CopyIcon size={13} />}
                  </button>
                  <button
                    onClick={() => void handleRevoke(inv.uri)}
                    className="btn btn-ghost btn-xs rounded-lg"
                    title="Revoke invitation"
                    aria-label="Revoke invitation"
                  >
                    <TrashIcon size={13} />
                  </button>
                </div>
              );
            })}
          </div>

          <div className="modal-action justify-center">
            <button onClick={dismiss} className="btn btn-ghost btn-sm rounded-lg text-xs">
              Close
            </button>
          </div>
        </div>
        <form method="dialog" className="modal-backdrop">
          <button aria-label="Close">close</button>
        </form>
      </dialog>
    );
  },
);
