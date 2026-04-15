import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import type { WorkspaceMember } from "@opake/sdk";
import { DestructiveConfirmation } from "@/components/DestructiveConfirmation";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { OpakeLogoSquares } from "@/components/OpakeLogoSquares";
import { Breadcrumbs, BreadcrumbActive } from "@/components/cabinet/Breadcrumbs";
import { AddMemberDialog, type AddMemberDialogHandle } from "@/components/cabinet/AddMemberDialog";
import { getOpake, useAuthStore } from "@/stores/auth";
import { useWorkspaceStore } from "@/stores/workspace";
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

  // Workspace metadata from the sidebar store (loaded on cabinet mount)
  const workspace = useWorkspaceStore((s) =>
    Object.values(s.workspaces).find((w) => rkeyFromUri(w.uri) === rkey),
  );
  const keyringUri = workspace?.uri ?? null;

  // Members + key material — fetched on-demand for this page
  const [rawMembers, setRawMembers] = useState<readonly WorkspaceMember[]>([]);
  const [groupKey, setGroupKey] = useState<Uint8Array | null>(null);
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

  const reloadMembersAndKey = useCallback(async (uri: string) => {
    const opake = getOpake();
    const ms = await opake.listWorkspaceMembers(uri);
    setRawMembers(ms);
    // Re-unwrap — owner rotations change the key, additions don't
    const key = await opake.unwrapGroupKey(ms);
    setGroupKey(key);
  }, []);

  // Refresh after a member mutation. Pulls the fresh member list + key
  // from the PDS (no appview lag on that path), then optimistically
  // patches the sidebar's member count. We deliberately skip calling
  // `loadWorkspaces()` because it would re-fetch via appview and clobber
  // any optimistic metadata patch from a prior save. The visibility
  // listener reconciles the full workspace record eventually.
  const refreshAfterMemberChange = useCallback(
    async (uri: string, memberCountDelta: number) => {
      await reloadMembersAndKey(uri);
      useWorkspaceStore.setState((draft) => {
        if (!(uri in draft.workspaces)) return;
        const w = draft.workspaces[uri];
        draft.workspaces[uri] = {
          ...w,
          memberCount: Math.max(0, w.memberCount + memberCountDelta),
        };
      });
    },
    [reloadMembersAndKey],
  );

  // Initial load
  useEffect(() => {
    if (!keyringUri) return;
    const uri = keyringUri;
    const done = loading("workspace-settings-load");
    (async () => {
      try {
        await reloadMembersAndKey(uri);
        setLoadError(null);
      } catch (err) {
        setLoadError(err instanceof Error ? err.message : "Failed to load members");
      } finally {
        done();
      }
    })().catch((err: unknown) => {
      console.error("[workspace-settings] load failed:", err);
    });
  }, [keyringUri, reloadMembersAndKey]);

  // Profile resolution
  useEffect(() => {
    if (members.length === 0) return;
    const unresolved = members.map((m) => m.did).filter((did) => !(did in profiles));
    if (unresolved.length === 0) return;
    void Promise.all(
      unresolved.map(async (did) => {
        const profile = await resolveMemberProfile(did);
        setProfiles((prev) => ({ ...prev, [did]: profile }));
      }),
    );
  }, [members, profiles]);

  // -----------------------------------------------------------------
  // Handlers
  // -----------------------------------------------------------------

  const handleSaveMetadata = useCallback(() => {
    if (!keyringUri || !groupKey || !name.trim()) return;
    const uri = keyringUri;
    const done = loading("save-workspace-metadata");
    (async () => {
      try {
        const nextName = nameOverride != null ? name.trim() : undefined;
        const nextDesc = descriptionOverride != null ? description.trim() : undefined;
        const nextIcon = iconOverride ?? undefined;
        await getOpake().updateWorkspaceMetadata(uri, groupKey, {
          name: nextName,
          description: nextDesc,
          icon: nextIcon,
        });
        toastSuccess("Workspace updated");

        // Optimistic patch: the appview has a 4s cursor lag, so
        // `loadWorkspaces` would return stale data and blank the form.
        // Update the store locally with the known-saved values; the
        // visibility listener (or a future SSE keyring callback) will
        // eventually reconcile against the real record.
        useWorkspaceStore.setState((draft) => {
          if (!(uri in draft.workspaces)) return;
          const w = draft.workspaces[uri];
          draft.workspaces[uri] = {
            ...w,
            name: nextName ?? w.name,
            description: nextDesc ?? w.description,
            icon: nextIcon ?? w.icon,
          };
        });
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
  }, [keyringUri, groupKey, name, description, nameOverride, descriptionOverride, iconOverride]);

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
      if (!keyringUri || !groupKey) return;
      if (confirmingRemove !== memberDid) {
        setConfirmingRemove(memberDid);
        return;
      }
      setConfirmingRemove(null);

      const uri = keyringUri;
      const done = loading("remove-workspace-member");
      (async () => {
        try {
          const result = await getOpake().removeWorkspaceMember(uri, groupKey, memberDid);
          toastSuccess(result.proposed ? "Removal proposed" : "Member removed, key rotated");
          await refreshAfterMemberChange(uri, -1);
        } catch (err) {
          toastError(err instanceof Error ? err.message : "Failed to remove member");
        } finally {
          done();
        }
      })().catch((err: unknown) => {
        console.error("[workspace-settings] remove failed:", err);
      });
    },
    [keyringUri, groupKey, confirmingRemove, refreshAfterMemberChange],
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
        await useWorkspaceStore.getState().loadWorkspaces();
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
    // TODO: workspace deletion not yet implemented in core. Needs purge of
    // docs + directories + keyring record as a single transaction. For now
    // the confirmation just navigates away — no destructive action taken.
    toastError("Delete workspace not yet implemented");
    void navigate({ to: "/cabinet/files" });
  }, [keyringUri, navigate]);

  const handleAddMember = useCallback(
    (handle: string, memberRole: WorkspaceRole) => {
      if (!keyringUri || !groupKey) return;
      const uri = keyringUri;
      const done = loading("add-workspace-member");
      (async () => {
        try {
          const opake = getOpake();
          const identity = await opake.resolveIdentity(handle);
          await opake.addWorkspaceMember(
            uri,
            groupKey,
            identity.did,
            identity.publicKey,
            memberRole,
          );
          toastSuccess(`Added ${identity.handle ?? identity.did}`);
          await refreshAfterMemberChange(uri, 1);
        } catch (err) {
          toastError(err instanceof Error ? err.message : "Failed to add member");
        } finally {
          done();
        }
      })().catch((err: unknown) => {
        console.error("[workspace-settings] add failed:", err);
      });
    },
    [keyringUri, groupKey, refreshAfterMemberChange],
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
          await refreshAfterMemberChange(uri, 0);
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
                    {name[0].toUpperCase()}
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
                disabled={!name.trim() || !groupKey}
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
          {loadError && <p className="text-caption text-error mb-2">{loadError}</p>}
          <ul className="space-y-1" aria-label="Member list">
            {members.map((member) => (
              <MemberRow
                key={member.did}
                member={member}
                profile={member.did in profiles ? (profiles[member.did] ?? null) : null}
                isMe={member.did === myDid}
                isManager={isManager}
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
