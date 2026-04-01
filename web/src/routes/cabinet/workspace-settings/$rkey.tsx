import { useCallback, useEffect, useState } from "react";
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import {
  ArrowLeftIcon,
  UsersIcon,
  CrownIcon,
  PencilSimpleIcon,
  EyeIcon,
  UserMinusIcon,
  UserPlusIcon,
  SignOutIcon,
  TrashIcon,
} from "@phosphor-icons/react";
import { DestructiveConfirmation } from "@/components/DestructiveConfirmation";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { OpakeLogoSquares } from "@/components/OpakeLogoSquares";
import { Breadcrumbs, BreadcrumbActive } from "@/components/cabinet/Breadcrumbs";
import { AddMemberDialog, type AddMemberDialogHandle } from "@/components/cabinet/AddMemberDialog";
import { useKeyringStore } from "@/stores/keyring";
import { useAuthStore } from "@/stores/auth";
import { rkeyFromUri, didFromUri } from "@/lib/atUri";
import { resolveMemberProfile, type MemberProfile } from "@/lib/profileResolution";
import type { WorkspaceRole, KeyringMemberEntry } from "@/lib/workspaceSchemas";
import { useRef } from "react";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ROLE_ICON: Readonly<Record<WorkspaceRole, typeof CrownIcon>> = {
  manager: CrownIcon,
  editor: PencilSimpleIcon,
  viewer: EyeIcon,
};

const ROLE_OPTIONS: readonly WorkspaceRole[] = ["manager", "editor", "viewer"];

// ---------------------------------------------------------------------------
// Settings page
// ---------------------------------------------------------------------------

function WorkspaceSettingsPage() {
  const { rkey } = Route.useParams();
  const navigate = useNavigate();
  const addMemberDialogRef = useRef<AddMemberDialogHandle>(null);

  const keyrings = useKeyringStore((s) => s.keyrings);
  const keyringsLoaded = useKeyringStore((s) => s.keyringsLoaded);
  const loadKeyrings = useKeyringStore((s) => s.loadKeyrings);
  const addMember = useKeyringStore((s) => s.addMember);
  const removeMember = useKeyringStore((s) => s.removeMember);
  const updateWorkspaceMetadata = useKeyringStore((s) => s.updateWorkspaceMetadata);
  const updateMemberRole = useKeyringStore((s) => s.updateMemberRole);
  const leaveWorkspace = useKeyringStore((s) => s.leaveWorkspace);
  const myRole = useKeyringStore((s) => s.myRole);

  const session = useAuthStore((s) => s.session);
  const myDid = session.status === "active" ? session.did : null;

  // Ensure keyrings loaded (handles direct URL navigation)
  useEffect(() => {
    if (!keyringsLoaded) void loadKeyrings();
  }, [keyringsLoaded, loadKeyrings]);

  const keyring = Object.values(keyrings).find((k) => rkeyFromUri(k.uri) === rkey);
  const keyringUri = keyring?.uri ?? null;
  const role = keyringUri ? myRole(keyringUri) : null;
  const isManager = role === "manager";

  // Editable metadata — keyed by keyring URI so state resets on workspace switch
  const seedName = keyring?.name ?? "";
  const seedDescription = keyring?.description ?? "";
  const [nameOverride, setNameOverride] = useState<string | null>(null);
  const [descriptionOverride, setDescriptionOverride] = useState<string | null>(null);
  const [iconOverride, setIconOverride] = useState<string | null>(null);
  const iconInputRef = useRef<HTMLInputElement>(null);

  const name = nameOverride ?? seedName;
  const description = descriptionOverride ?? seedDescription;
  const icon = iconOverride ?? keyring?.icon ?? null;
  const metaDirty = nameOverride !== null || descriptionOverride !== null || iconOverride !== null;

  // Profile resolution
  const [profiles, setProfiles] = useState<Readonly<Record<string, MemberProfile>>>({});
  useEffect(() => {
    if (!keyring) return;
    const unresolved = keyring.members.map((m) => m.did).filter((did) => !(did in profiles));
    if (unresolved.length === 0) return;
    void Promise.all(
      unresolved.map(async (did) => {
        const profile = await resolveMemberProfile(did);
        setProfiles((prev) => ({ ...prev, [did]: profile }));
      }),
    );
  }, [keyring, profiles]);

  // Confirm state for remove + leave + delete
  const [confirmingRemove, setConfirmingRemove] = useState<string | null>(null);
  const [confirmingLeave, setConfirmingLeave] = useState(false);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);

  const handleSaveMetadata = useCallback(() => {
    if (!keyringUri || !name.trim()) return;
    void updateWorkspaceMetadata(
      keyringUri,
      nameOverride != null ? name.trim() : undefined,
      descriptionOverride != null ? description.trim() : undefined,
      iconOverride,
    );
    setNameOverride(null);
    setDescriptionOverride(null);
    setIconOverride(null);
  }, [
    keyringUri,
    name,
    description,
    nameOverride,
    descriptionOverride,
    iconOverride,
    updateWorkspaceMetadata,
  ]);

  const handleIconSelected = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    // Read file → resize to 128x128 via canvas → base64 data URL
    const reader = new FileReader();
    // eslint-disable-next-line functional/immutable-data -- FileReader callback pattern
    reader.onload = () => {
      const img = new Image();
      // eslint-disable-next-line functional/immutable-data -- Image callback pattern
      img.onload = () => {
        const canvas = document.createElement("canvas");
        // eslint-disable-next-line functional/immutable-data
        canvas.width = 128;
        // eslint-disable-next-line functional/immutable-data
        canvas.height = 128;
        const ctx = canvas.getContext("2d");
        if (!ctx) return;
        ctx.drawImage(img, 0, 0, 128, 128);
        const dataUrl = canvas.toDataURL("image/png");
        setIconOverride(dataUrl.split(",")[1]);
      };
      // eslint-disable-next-line functional/immutable-data -- Image src setter triggers load
      img.src = reader.result as string;
    };
    reader.readAsDataURL(file);
  }, []);

  const handleRemove = useCallback(
    (memberDid: string) => {
      if (!keyringUri) return;
      if (confirmingRemove === memberDid) {
        void removeMember(keyringUri, memberDid);
        setConfirmingRemove(null);
      } else {
        setConfirmingRemove(memberDid);
      }
    },
    [keyringUri, confirmingRemove, removeMember],
  );

  const handleLeave = useCallback(() => {
    if (!keyringUri) return;
    if (confirmingLeave) {
      void leaveWorkspace(keyringUri).then(() => {
        void navigate({ to: "/cabinet/files" });
      });
    } else {
      setConfirmingLeave(true);
    }
  }, [keyringUri, confirmingLeave, leaveWorkspace, navigate]);

  const isOwner = keyring ? didFromUri(keyring.uri) === myDid : false;

  const handleDeleteWorkspace = useCallback(() => {
    if (!keyringUri) return;
    // [NOI FEEDBACK PLS] — deleteWorkspace not implemented in core yet.
    // Needs: delete all workspace docs + directories + keyring record.
    void navigate({ to: "/cabinet/files" });
  }, [keyringUri, navigate]);

  const handleAddMember = useCallback(
    (handle: string, memberRole: WorkspaceRole) => {
      if (!keyringUri) return;
      void addMember(keyringUri, handle, memberRole);
    },
    [keyringUri, addMember],
  );

  const workspaceName = keyring?.name ?? "Workspace";

  const breadcrumbs = (
    <Breadcrumbs>
      <li>
        <Link to="/cabinet/workspace/$rkey" params={{ rkey }} className="text-text-faint">
          <UsersIcon size={14} className="mr-1.5 inline md:hidden" />
          {workspaceName}
        </Link>
      </li>
      <BreadcrumbActive>Settings</BreadcrumbActive>
    </Breadcrumbs>
  );

  const toolbar = (
    <Link
      to="/cabinet/workspace/$rkey"
      params={{ rkey }}
      className="btn btn-ghost btn-sm gap-1.5 rounded-lg text-xs"
    >
      <ArrowLeftIcon size={13} />
      Back
    </Link>
  );

  if (!keyring) {
    return (
      <PanelShell depth={1} breadcrumbs={breadcrumbs} footer="Loading…">
        <div className="flex h-full items-center justify-center">
          <OpakeLogoSquares size="lg" loading />
        </div>
      </PanelShell>
    );
  }

  return (
    <PanelShell depth={1} breadcrumbs={breadcrumbs} toolbar={toolbar} footer="End-to-end encrypted">
      <div className="mx-auto max-w-lg space-y-8 px-6 py-6">
        {/* Metadata */}
        <section>
          <h2 className="text-base-content mb-3 text-sm font-semibold">Workspace</h2>
          <div className="space-y-3">
            <div className="flex items-center gap-4">
              <button
                onClick={() => iconInputRef.current?.click()}
                className="group relative shrink-0"
                aria-label="Change workspace icon"
              >
                {icon ? (
                  <img
                    src={`data:image/png;base64,${icon}`}
                    alt=""
                    className="size-14 rounded-xl object-cover"
                  />
                ) : (
                  <div className="bg-accent text-primary flex size-14 items-center justify-center rounded-xl text-xl font-semibold">
                    {name[0].toUpperCase()}
                  </div>
                )}
                <div className="bg-base-content/50 absolute inset-0 flex items-center justify-center rounded-xl opacity-0 transition-opacity group-hover:opacity-100">
                  <PencilSimpleIcon size={16} className="text-white" />
                </div>
              </button>
              <input
                ref={iconInputRef}
                type="file"
                accept="image/*"
                onChange={handleIconSelected}
                className="sr-only"
                aria-hidden="true"
              />
              <div className="flex-1">
                <span className="text-ui text-base-content block">{name || "Unnamed"}</span>
                <span className="text-caption text-text-faint">
                  {description || "No description"}
                </span>
              </div>
            </div>
            <div>
              <label htmlFor="ws-name" className="text-caption text-text-muted mb-1 block">
                Name
              </label>
              <input
                id="ws-name"
                type="text"
                value={name}
                onChange={(e) => setNameOverride(e.target.value)}
                className="input input-bordered input-sm border-base-300/50 w-full rounded-lg text-xs"
              />
            </div>
            <div>
              <label htmlFor="ws-desc" className="text-caption text-text-muted mb-1 block">
                Description
              </label>
              <textarea
                id="ws-desc"
                value={description}
                onChange={(e) => setDescriptionOverride(e.target.value)}
                className="textarea textarea-bordered textarea-sm border-base-300/50 w-full rounded-lg text-xs"
                rows={2}
              />
            </div>
            {metaDirty && (
              <button
                onClick={handleSaveMetadata}
                disabled={!name.trim()}
                className="btn btn-primary btn-sm rounded-lg text-xs"
              >
                Save changes
              </button>
            )}
          </div>
        </section>

        {/* Members */}
        <section>
          <div className="mb-3 flex items-center justify-between">
            <h2 className="text-base-content text-sm font-semibold">
              Members ({keyring.members.length})
            </h2>
            {isManager && (
              <button
                onClick={() => addMemberDialogRef.current?.show()}
                className="btn btn-ghost btn-xs gap-1 rounded-lg"
              >
                <UserPlusIcon size={13} />
                Add
              </button>
            )}
          </div>
          <ul className="space-y-1" aria-label="Member list">
            {keyring.members.map((member) => (
              <MemberRow
                key={member.did}
                member={member}
                profile={member.did in profiles ? profiles[member.did] : null}
                isMe={member.did === myDid}
                isManager={isManager}
                confirmingRemove={confirmingRemove === member.did}
                onRemove={() => handleRemove(member.did)}
                onRoleChange={(newRole) => {
                  if (!keyringUri) return;
                  void updateMemberRole(keyringUri, member.did, newRole);
                }}
              />
            ))}
          </ul>
        </section>

        {/* Danger zone */}
        <section>
          <h2 className="text-error mb-3 text-sm font-semibold">Danger zone</h2>
          <div className="border-error/20 space-y-4 rounded-lg border p-4">
            {/* Leave */}
            <div>
              <p className="text-caption text-text-muted mb-2">
                Leave this workspace. Your access will be revoked.
              </p>
              <button
                onClick={handleLeave}
                className="btn btn-confirm-danger btn-sm gap-1.5 rounded-lg text-xs"
                data-confirming={confirmingLeave || undefined}
              >
                <SignOutIcon size={13} />
                {confirmingLeave ? "Click again to confirm" : "Leave workspace"}
              </button>
            </div>

            {/* Delete — owner only */}
            {isOwner && (
              <div>
                <p className="text-caption text-text-muted mb-2">
                  Permanently delete this workspace and all its files. This cannot be undone.
                </p>
                {showDeleteConfirm ? (
                  <DestructiveConfirmation
                    phrase={`I want to delete ${workspaceName} and all its data`}
                    onConfirm={handleDeleteWorkspace}
                  />
                ) : (
                  <button
                    onClick={() => setShowDeleteConfirm(true)}
                    className="btn btn-ghost btn-sm gap-1.5 rounded-lg text-xs"
                  >
                    <TrashIcon size={13} />
                    Delete workspace
                  </button>
                )}
              </div>
            )}
          </div>
        </section>
      </div>

      <AddMemberDialog ref={addMemberDialogRef} onConfirm={handleAddMember} />
    </PanelShell>
  );
}

// ---------------------------------------------------------------------------
// Member row
// ---------------------------------------------------------------------------

function MemberRow({
  member,
  profile,
  isMe,
  isManager,
  confirmingRemove,
  onRemove,
  onRoleChange,
}: {
  readonly member: KeyringMemberEntry;
  readonly profile: MemberProfile | null;
  readonly isMe: boolean;
  readonly isManager: boolean;
  readonly confirmingRemove: boolean;
  readonly onRemove: () => void;
  readonly onRoleChange: (role: WorkspaceRole) => void;
}) {
  const RoleIcon = ROLE_ICON[member.role];
  const displayName = profile?.handle ?? member.did;
  const canRemove = isManager && !isMe;
  const canChangeRole = isManager && !isMe;

  return (
    <li className="flex items-center gap-3 rounded-lg px-3 py-2.5">
      {profile?.avatarUrl ? (
        <img src={profile.avatarUrl} alt="" className="size-8 shrink-0 rounded-full object-cover" />
      ) : (
        <div className="bg-accent text-primary text-micro flex size-8 shrink-0 items-center justify-center rounded-full font-semibold">
          {displayName[0].toUpperCase()}
        </div>
      )}
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="text-ui text-base-content truncate">
          {displayName}
          {isMe && <span className="text-text-faint ml-1.5 text-[10px]">(you)</span>}
        </span>
        {canChangeRole ? (
          <select
            value={member.role}
            onChange={(e) => onRoleChange(e.target.value as WorkspaceRole)}
            className="select select-xs text-caption text-base-content bg-base-100 w-24 rounded-lg"
            aria-label={`Role for ${displayName}`}
          >
            {ROLE_OPTIONS.map((r) => (
              <option key={r} value={r}>
                {r.charAt(0).toUpperCase() + r.slice(1)}
              </option>
            ))}
          </select>
        ) : (
          <span className="text-caption text-text-faint flex items-center gap-1">
            <RoleIcon size={10} />
            {member.role.charAt(0).toUpperCase() + member.role.slice(1)}
          </span>
        )}
      </div>
      {canRemove && (
        <button
          onClick={onRemove}
          className="btn btn-confirm-danger btn-xs rounded-lg"
          data-confirming={confirmingRemove || undefined}
          title={confirmingRemove ? "Click again to confirm" : "Remove member"}
          aria-label={`Remove ${displayName}`}
        >
          <UserMinusIcon size={14} />
          {confirmingRemove && <span className="text-[10px]">confirm?</span>}
        </button>
      )}
    </li>
  );
}

export const Route = createFileRoute("/cabinet/workspace-settings/$rkey")({
  component: WorkspaceSettingsPage,
});
