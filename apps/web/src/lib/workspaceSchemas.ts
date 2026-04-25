// Web-side view models for workspace member management.
//
// The SDK's `WorkspaceMember` type carries the full wrapped-key structure
// required for crypto operations. UI components only need a flat
// `{ did, role }` view — this module bridges the two and re-exports
// `WorkspaceRole` so components don't reach across the SDK boundary.

import type { WorkspaceMember, WorkspaceRole } from "@opake/sdk";

export type { WorkspaceRole };

/** Flat member shape for UI rendering — just DID and role, no crypto material. */
export interface KeyringMemberEntry {
  readonly did: string;
  readonly role: WorkspaceRole;
}

/** Project a raw `WorkspaceMember` onto the UI-facing `KeyringMemberEntry`. */
export function toMemberEntry(member: WorkspaceMember): KeyringMemberEntry {
  return {
    did: member.wrappedKey.did,
    role: member.role,
  };
}
