import { getOpakeWorker } from "@/lib/worker";

export interface MemberProfile {
  readonly handle: string | null;
  readonly avatarUrl: string | null;
}

/** Fetch a member's profile record directly from their PDS (pure atproto). */
export async function resolveMemberProfile(did: string): Promise<MemberProfile> {
  try {
    const worker = getOpakeWorker();
    const resolved = (await worker.resolveIdentity(did)) as {
      did: string;
      handle?: string;
      pds_url: string;
    };
    const handle = resolved.handle ?? null;
    const pdsUrl = resolved.pds_url;

    const profileRes = await fetch(
      `${pdsUrl}/xrpc/com.atproto.repo.getRecord?repo=${encodeURIComponent(did)}&collection=app.bsky.actor.profile&rkey=self`,
    );
    if (!profileRes.ok) return { handle, avatarUrl: null };

    const record = (await profileRes.json()) as {
      value?: { avatar?: { ref?: { $link?: string } } };
    };
    const cid = record.value?.avatar?.ref?.$link;
    if (!cid) return { handle, avatarUrl: null };

    const avatarUrl = `${pdsUrl}/xrpc/com.atproto.sync.getBlob?did=${encodeURIComponent(did)}&cid=${encodeURIComponent(cid)}`;
    return { handle, avatarUrl };
  } catch {
    return { handle: null, avatarUrl: null };
  }
}
