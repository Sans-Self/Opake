// Auth error detection + session expiry — shared across worker proxy,
// service worker, and service worker registration.

/** Error message patterns that indicate an auth/session failure from core. */
const AUTH_ERROR_PATTERNS = [
  "authentication failed",
  "token refresh failed",
  "no session for",
  "no default account",
] as const;

/** Check if an error from core is an authentication/session failure. */
export function isAuthError(error: unknown): boolean {
  if (!(error instanceof Error)) return false;
  const msg = error.message.toLowerCase();
  return AUTH_ERROR_PATTERNS.some((p) => msg.includes(p));
}

/**
 * Invalidate the in-memory session without touching IndexedDB.
 * Keys and account config survive — re-login restores the session.
 * Lazy-imports auth store to avoid circular dependencies.
 */
export function expireSession(): void {
  void import("@/stores/auth").then(({ useAuthStore }) => {
    const { session } = useAuthStore.getState();
    if (session.status !== "active") return;

    const did = session.did;
    console.warn("[auth] session expired — clearing session and redirecting to login");
    useAuthStore.setState({ session: { status: "none" }, identity: { status: "unchecked" } });

    // Clear stale session from IndexedDB. Identity and account config survive.
    void import("@/lib/indexeddbStorage").then(({ storage }) =>
      storage
        .deleteSession(did)
        .catch((e: unknown) => console.warn("[auth] failed to clear session:", e)),
    );

    // beforeLoad only fires on navigation, not state changes — force redirect
    if (window.location.pathname.startsWith("/cabinet")) {
      window.location.href = "/devices/login";
    }
  });
}
