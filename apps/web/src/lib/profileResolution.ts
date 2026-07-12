// Best-effort resolution of a DID → display profile (handle + avatar).
//
// Uses Bluesky's public indexer so we don't pay DPoP setup just to render
// a member list. If the account isn't on bsky, or the fetch fails for
// any reason, we degrade gracefully to a null profile and callers fall
// back to the raw DID as display text.
//
// Results are memoized per DID for the lifetime of the page — the bsky
// indexer is already cached at the CDN, but coalescing local duplicates
// avoids N parallel fetches when a workspace has many members.

// Bluesky appview for cosmetic member-profile (handle/avatar) lookups.
// Overridable via VITE_BSKY_APPVIEW_URL so the dev-env can neutralize it
// (empty → same-origin, 404 → graceful null); left unset it targets the live
// public appview. Without this gate the fetch escapes to public.api.bsky.app,
// which the e2e hermeticity blockade fails. Mirrors stores/auth.ts.
const PUBLIC_API =
  (import.meta.env.VITE_BSKY_APPVIEW_URL as string | undefined) ?? "https://public.api.bsky.app";

export interface MemberProfile {
  readonly handle: string | null;
  readonly avatarUrl: string | null;
}

interface RawProfile {
  readonly handle?: string;
  readonly avatar?: string;
}

const cache = new Map<string, Promise<MemberProfile | null>>();

/** Only accept avatar URLs from known Bluesky CDN origins. */
function isSafeCdnUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "https:" && parsed.hostname.endsWith(".bsky.app");
  } catch {
    return false;
  }
}

async function fetchProfile(did: string): Promise<MemberProfile | null> {
  try {
    const response = await fetch(
      `${PUBLIC_API}/xrpc/app.bsky.actor.getProfile?actor=${encodeURIComponent(did)}`,
    );
    if (!response.ok) return null;
    const raw = (await response.json()) as RawProfile;
    return {
      handle: raw.handle ?? null,
      avatarUrl: raw.avatar && isSafeCdnUrl(raw.avatar) ? raw.avatar : null,
    };
  } catch {
    return null;
  }
}

/**
 * Resolve a DID's display profile. Memoized for the page lifetime.
 *
 * @param did - The member DID to look up.
 * @returns Profile with handle + avatar, or `null` if resolution failed.
 */
export function resolveMemberProfile(did: string): Promise<MemberProfile | null> {
  const cached = cache.get(did);
  if (cached) return cached;
  const pending = fetchProfile(did);
  // Module-local cache — the immutability rule misreads this as a domain
  // concern. It's a fetch-dedup singleton with page lifetime.
  // eslint-disable-next-line functional/immutable-data
  cache.set(did, pending);
  return pending;
}
