import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, createLazyFileRoute } from "@tanstack/react-router";
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
import type { WorkspaceMember } from "@opake/sdk";
import { useWorkspaces } from "@opake/react";
import { DestructiveConfirmation } from "@/components/DestructiveConfirmation";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { OpakeLogoSquares } from "@/components/OpakeLogoSquares";
import { Breadcrumbs, BreadcrumbActive } from "@/components/cabinet/Breadcrumbs";
import { AddMemberDialog, type AddMemberDialogHandle } from "@/components/cabinet/AddMemberDialog";
import { getOpake, useAuthStore } from "@/stores/auth";
import { toastError, toastSuccess } from "@/stores/toast";
import { rkeyFromUri } from "@/lib/atUri";
import { resolveMemberProfile, type MemberProfile } from "@/lib/profileResolution";
import { toMemberEntry, type KeyringMemberEntry, type WorkspaceRole } from "@/lib/workspaceSchemas";
import { loading } from "@/stores/app";

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
  const iconInputRef = useRef<HTMLInputElement>(null);

  // Current session
  const session = useAuthStore((s) => s.session);
  const myDid = session.status === "active" ? session.did : null;

  // Workspace metadata — mirrors whatever the WorkspaceKeeper has loaded.
  const { data: workspaces } = useWorkspaces();
  const workspace = workspaces.find((w) => rkeyFromUri(w.uri) === rkey);
  const keyringUri = workspace?.uri ?? null;

  // Members — fetched on-demand for this page. The workspace group key
  // never enters JS state; every mutation re-resolves it inside WASM via
  // the keyring URI.
  const [rawMembers, setRawMembers] = useState<readonly WorkspaceMember[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const members: readonly KeyringMemberEntry[] = useMemo(
    () => rawMembers.map(toMemberEntry),
    [rawMembers],
  );

  // Editable metadata — override pattern lets us track "dirty" without
  // wiping the seed when the underlying record re-renders
  const seedName = workspace?.name ?? "";
  const seedDescription = workspace?.description ?? "";
  const seedIcon = workspace?.icon ?? null;
  const [nameOverride, setNameOverride] = useState<string | null>(null);
  const [descriptionOverride, setDescriptionOverride] = useState<string | null>(null);
  const [iconOverride, setIconOverride] = useState<string | null>(null);
  const name = nameOverride ?? seedName;
  const description = descriptionOverride ?? seedDescription;
  const icon = iconOverride ?? seedIcon;
  const metaDirty = nameOverride !== null || descriptionOverride !== null || iconOverride !== null;

  // Profile resolution for member rows
  const [profiles, setProfiles] = useState<Readonly<Record<string, MemberProfile | null>>>({});

  // Confirm state (two-click for remove + leave, phrase-typing for delete)
  const [confirmingRemove, setConfirmingRemove] = useState<string | null>(null);
  const [confirmingLeave, setConfirmingLeave] = useState(false);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);

  const role = useMemo(() => members.find((m) => m.did === myDid)?.role ?? null, [members, myDid]);
  const isManager = role === "manager";
  const isOwner = myDid !== null && myDid === workspace?.ownerDid;
  const canManage = isOwner || isManager;

  // -----------------------------------------------------------------
  // Loaders
  // -----------------------------------------------------------------

  const reloadMembers = useCallback(async (uri: string) => {
    const ms = await getOpake().listWorkspaceMembers(uri);
    setRawMembers(ms);
  }, []);

  // Refresh the page-local member list after a mutation. The sidebar's
  // workspace record updates on its own via the SSE echo →
  // WorkspaceKeeper path, so we don't patch the store here.
  const refreshAfterMemberChange = useCallback(
    async (uri: string) => {
      await reloadMembers(uri);
    },
    [reloadMembers],
  );

  // Initial load
  useEffect(() => {
    if (!keyringUri) return;
    const uri = keyringUri;
    const done = loading("workspace-settings-load");
    (async () => {
      try {
        await reloadMembers(uri);
        setLoadError(null);
      } catch (err) {
        setLoadError(err instanceof Error ? err.message : "Failed to load members");
      } finally {
        done();
      }
    })().catch((err: unknown) => {
      console.error("[workspace-settings] load failed:", err);
    });
  }, [keyringUri, reloadMembers]);

  // Profile resolution — depends only on `members`, not `profiles`.
  // `resolveMemberProfile` memoizes per-DID for the page lifetime, so
  // re-fetches on member list changes are free for already-resolved DIDs.
  // The functional setter avoids a `profiles` dep (which would cause
  // N+1 re-runs as each resolution triggers a new reference).
  useEffect(() => {
    if (members.length === 0) return;
    members.forEach((m) => {
      void resolveMemberProfile(m.did).then((resolved) => {
        setProfiles((prev) => (prev[m.did] === resolved ? prev : { ...prev, [m.did]: resolved }));
      });
    });
  }, [members]);

  // -----------------------------------------------------------------
  // Handlers
  // -----------------------------------------------------------------

  const handleSaveMetadata = useCallback(() => {
    if (!keyringUri || !name.trim()) return;
    const uri = keyringUri;
    const done = loading("save-workspace-metadata");
    (async () => {
      try {
        const nextName = nameOverride != null ? name.trim() : undefined;
        const nextDesc = descriptionOverride != null ? description.trim() : undefined;
        const nextIcon = iconOverride ?? undefined;
        await getOpake().updateWorkspaceMetadata(uri, {
          name: nextName,
          description: nextDesc,
          icon: nextIcon,
        });
        toastSuccess("Workspace updated");

        // The SSE echo for this keyring write will fire KeyringUpsert,
        // which the WorkspaceKeeper applies → watcher → store update.
        // No manual store patch needed.
        setNameOverride(null);
        setDescriptionOverride(null);
        setIconOverride(null);
      } catch (err) {
        toastError(err instanceof Error ? err.message : "Failed to update workspace");
      } finally {
        done();
      }
    })().catch((err: unknown) => {
      console.error("[workspace-settings] save failed:", err);
    });
  }, [keyringUri, name, description, nameOverride, descriptionOverride, iconOverride]);

  const handleIconSelected = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    // FileReader + canvas pipeline: decode → resize to 128×128 → base64
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
        setIconOverride(dataUrl.split(",")[1] ?? null);
      };
      // eslint-disable-next-line functional/immutable-data -- Image src setter triggers load
      img.src = reader.result as string;
    };
    reader.readAsDataURL(file);
  }, []);

  const handleRemove = useCallback(
    (memberDid: string) => {
      if (!keyringUri) return;
      if (confirmingRemove !== memberDid) {
        setConfirmingRemove(memberDid);
        return;
      }
      setConfirmingRemove(null);

      const uri = keyringUri;
      const done = loading("remove-workspace-member");
      (async () => {
        try {
          const result = await getOpake().removeWorkspaceMember(uri, memberDid);
          toastSuccess(result.proposed ? "Removal proposed" : "Member removed, key rotated");
          await refreshAfterMemberChange(uri);
        } catch (err) {
          toastError(err instanceof Error ? err.message : "Failed to remove member");
        } finally {
          done();
        }
      })().catch((err: unknown) => {
        console.error("[workspace-settings] remove failed:", err);
      });
    },
    [keyringUri, confirmingRemove, refreshAfterMemberChange],
  );

  const handleLeave = useCallback(() => {
    if (!keyringUri) return;
    if (!confirmingLeave) {
      setConfirmingLeave(true);
      return;
    }
    const uri = keyringUri;
    const done = loading("leave-workspace");
    (async () => {
      try {
        await getOpake().leaveWorkspace(uri);
        toastSuccess("Left workspace");
        // SSE echo (keyring:upsert without our DID, or keyring:delete
        // for an owner-side purge) removes the entry from the keeper.
        void navigate({ to: "/cabinet/files" });
      } catch (err) {
        toastError(err instanceof Error ? err.message : "Failed to leave workspace");
      } finally {
        done();
      }
    })().catch((err: unknown) => {
      console.error("[workspace-settings] leave failed:", err);
    });
  }, [keyringUri, confirmingLeave, navigate]);

  const handleDeleteWorkspace = useCallback(() => {
    if (!keyringUri) return;
    // Workspace deletion not yet implemented in core — needs purge of
    // docs + directories + keyring record as a single transaction.
    // Reset the confirmation UI without navigating (navigating after a
    // typed confirmation phrase makes it look like the delete succeeded).
    toastError("Workspace deletion is not yet available");
    setShowDeleteConfirm(false);
  }, [keyringUri]);

  const handleAddMember = useCallback(
    (handle: string, memberRole: WorkspaceRole) => {
      if (!keyringUri) return;
      const uri = keyringUri;
      const done = loading("add-workspace-member");
      (async () => {
        try {
          const opake = getOpake();
          const identity = await opake.resolveIdentity(handle);
          await opake.addWorkspaceMember(uri, identity.did, identity.publicKey, memberRole);
          toastSuccess(`Added ${identity.handle ?? identity.did}`);
          await refreshAfterMemberChange(uri);
        } catch (err) {
          toastError(err instanceof Error ? err.message : "Failed to add member");
        } finally {
          done();
        }
      })().catch((err: unknown) => {
        console.error("[workspace-settings] add failed:", err);
      });
    },
    [keyringUri, refreshAfterMemberChange],
  );

  const handleRoleChange = useCallback(
    (memberDid: string, newRole: WorkspaceRole) => {
      if (!keyringUri) return;
      const uri = keyringUri;
      const done = loading("update-member-role");
      (async () => {
        try {
          await getOpake().updateMemberRole(uri, memberDid, newRole);
          toastSuccess("Role updated");
          await refreshAfterMemberChange(uri);
        } catch (err) {
          toastError(err instanceof Error ? err.message : "Failed to update role");
        } finally {
          done();
        }
      })().catch((err: unknown) => {
        console.error("[workspace-settings] role change failed:", err);
      });
    },
    [keyringUri, refreshAfterMemberChange],
  );

  // -----------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------

  const workspaceName = workspace?.name ?? "Workspace";

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

  if (!workspace) {
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
                disabled={!canManage}
              >
                {icon ? (
                  <img
                    src={`data:image/png;base64,${icon}`}
                    alt=""
                    className="size-14 rounded-xl object-cover"
                  />
                ) : (
                  <div className="bg-accent text-primary flex size-14 items-center justify-center rounded-xl text-xl font-semibold">
                    {(name.charAt(0) || "?").toUpperCase()}
                  </div>
                )}
                {canManage && (
                  <div className="bg-base-content/50 absolute inset-0 flex items-center justify-center rounded-xl opacity-0 transition-opacity group-hover:opacity-100">
                    <PencilSimpleIcon size={16} className="text-white" />
                  </div>
                )}
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
                disabled={!canManage}
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
                disabled={!canManage}
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
            <h2 className="text-base-content text-sm font-semibold">Members ({members.length})</h2>
            {canManage && (
              <button
                onClick={() => addMemberDialogRef.current?.show()}
                className="btn btn-ghost btn-xs gap-1 rounded-lg"
              >
                <UserPlusIcon size={13} />
                Add
              </button>
            )}
          </div>
          {loadError && <p className="text-caption text-error mb-2">{loadError}</p>}
          <ul className="space-y-1" aria-label="Member list">
            {members.map((member) => (
              <MemberRow
                key={member.did}
                member={member}
                profile={member.did in profiles ? (profiles[member.did] ?? null) : null}
                isMe={member.did === myDid}
                canManage={canManage}
                confirmingRemove={confirmingRemove === member.did}
                onRemove={() => handleRemove(member.did)}
                onRoleChange={(newRole) => handleRoleChange(member.did, newRole)}
              />
            ))}
          </ul>
        </section>

        {/* Danger zone */}
        <section>
          <h2 className="text-error mb-3 text-sm font-semibold">Danger zone</h2>
          <div className="border-error/20 space-y-4 rounded-lg border p-4">
            {/* Leave — non-owners only */}
            {!isOwner && (
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
            )}

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
  canManage,
  confirmingRemove,
  onRemove,
  onRoleChange,
}: {
  readonly member: KeyringMemberEntry;
  readonly profile: MemberProfile | null;
  readonly isMe: boolean;
  readonly canManage: boolean;
  readonly confirmingRemove: boolean;
  readonly onRemove: () => void;
  readonly onRoleChange: (role: WorkspaceRole) => void;
}) {
  const RoleIcon = ROLE_ICON[member.role];
  const displayName = profile?.handle ?? member.did;
  const canRemove = canManage && !isMe;
  const canChangeRole = canManage && !isMe;

  return (
    <li className="flex items-center gap-3 rounded-lg px-3 py-2.5">
      {profile?.avatarUrl ? (
        <img src={profile.avatarUrl} alt="" className="size-8 shrink-0 rounded-full object-cover" />
      ) : (
        <div className="bg-accent text-primary text-micro flex size-8 shrink-0 items-center justify-center rounded-full font-semibold">
          {(displayName.charAt(0) || "?").toUpperCase()}
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

export const Route = createLazyFileRoute("/cabinet/workspace-settings/$rkey")({
  component: WorkspaceSettingsPage,
});
