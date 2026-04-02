import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import {
  UsersIcon,
  UserMinusIcon,
  CrownIcon,
  PencilSimpleIcon,
  EyeIcon,
} from "@phosphor-icons/react";
import { MODAL_TRANSITION_MS } from "@/components/ConfirmDialog";
import { useAuthStore } from "@/stores/auth";
import { resolveMemberProfile, type MemberProfile } from "@/lib/profileResolution";
import type { KeyringMemberEntry, WorkspaceRole } from "@/lib/workspaceSchemas";

export interface WorkspaceMembersDialogHandle {
  readonly show: () => void;
}

interface WorkspaceMembersDialogProps {
  readonly members: readonly KeyringMemberEntry[];
  readonly isManager: boolean;
  readonly onRemoveMember: (memberDid: string) => void;
  readonly onAddMember: () => void;
}

const ROLE_ICON: Readonly<Record<WorkspaceRole, typeof CrownIcon>> = {
  manager: CrownIcon,
  editor: PencilSimpleIcon,
  viewer: EyeIcon,
};

const ROLE_LABEL: Readonly<Record<WorkspaceRole, string>> = {
  manager: "Manager",
  editor: "Editor",
  viewer: "Viewer",
};

export const WorkspaceMembersDialog = forwardRef<
  WorkspaceMembersDialogHandle,
  WorkspaceMembersDialogProps
>(function WorkspaceMembersDialog({ members, isManager, onRemoveMember, onAddMember }, ref) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [confirmingRemove, setConfirmingRemove] = useState<string | null>(null);
  const [profiles, setProfiles] = useState<Readonly<Record<string, MemberProfile>>>({});
  const [visible, setVisible] = useState(false);

  const session = useAuthStore((s) => s.session);
  const myDid = session.status === "active" ? session.did : null;

  // Resolve profiles when dialog becomes visible
  useEffect(() => {
    if (!visible || members.length === 0) return;

    const unresolvedDids = members.map((m) => m.did).filter((did) => !(did in profiles));

    if (unresolvedDids.length === 0) return;

    void Promise.all(
      unresolvedDids.map(async (did) => {
        const profile = await resolveMemberProfile(did);
        setProfiles((prev) => ({ ...prev, [did]: profile }));
      }),
    );
  }, [visible, members, profiles]);

  const dismiss = useCallback(() => {
    dialogRef.current?.close();
    setTimeout(() => {
      setConfirmingRemove(null);
      setVisible(false);
    }, MODAL_TRANSITION_MS);
  }, []);

  const handleRemove = useCallback(
    (memberDid: string) => {
      if (confirmingRemove === memberDid) {
        onRemoveMember(memberDid);
        setConfirmingRemove(null);
        dismiss();
      } else {
        setConfirmingRemove(memberDid);
      }
    },
    [confirmingRemove, onRemoveMember, dismiss],
  );

  useImperativeHandle(ref, () => ({
    show: () => {
      setConfirmingRemove(null);
      setVisible(true);
      dialogRef.current?.showModal();
    },
  }));

  return (
    <dialog ref={dialogRef} className="modal" aria-label="Workspace members">
      <div className="modal-box max-w-sm">
        <div className="flex flex-col items-center gap-3 text-center">
          <div className="bg-accent/60 flex size-11 items-center justify-center rounded-full">
            <UsersIcon size={20} className="text-primary" />
          </div>
          <h3 className="text-base-content text-sm font-semibold">Members</h3>
        </div>

        <ul className="mt-4 flex flex-col gap-1" aria-label="Member list">
          {members.map((member) => {
            const RoleIcon = ROLE_ICON[member.role];
            const isMe = member.did === myDid;
            const canRemove = isManager && !isMe;
            const isConfirming = confirmingRemove === member.did;
            const profile = member.did in profiles ? profiles[member.did] : null;
            const displayName = profile?.handle ?? member.did;

            return (
              <li key={member.did} className="flex items-center gap-2.5 rounded-lg px-2.5 py-2">
                {profile?.avatarUrl ? (
                  <img
                    src={profile.avatarUrl}
                    alt=""
                    className="size-7 shrink-0 rounded-full object-cover"
                  />
                ) : (
                  <div className="bg-accent text-primary text-micro flex size-7 shrink-0 items-center justify-center rounded-full font-semibold">
                    {displayName[0].toUpperCase()}
                  </div>
                )}
                <div className="flex min-w-0 flex-1 flex-col">
                  <span className="text-ui text-base-content truncate">
                    {displayName}
                    {isMe && <span className="text-text-faint ml-1.5 text-[10px]">(you)</span>}
                  </span>
                  <span className="text-caption text-text-faint flex items-center gap-1">
                    <RoleIcon size={10} />
                    {ROLE_LABEL[member.role]}
                  </span>
                </div>
                {canRemove && (
                  <button
                    onClick={() => handleRemove(member.did)}
                    className={`btn btn-ghost btn-xs rounded-lg ${isConfirming ? "btn-error text-error" : ""}`}
                    title={isConfirming ? "Click again to confirm removal" : "Remove member"}
                    aria-label={`Remove ${displayName}`}
                  >
                    <UserMinusIcon size={14} />
                    {isConfirming && <span className="text-[10px]">confirm?</span>}
                  </button>
                )}
              </li>
            );
          })}
        </ul>

        <div className="modal-action justify-center gap-2">
          {isManager && (
            <button
              onClick={() => {
                dismiss();
                onAddMember();
              }}
              className="btn btn-primary btn-sm rounded-lg text-xs"
            >
              Add member
            </button>
          )}
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
});
