// Request persistent storage so the browser won't evict our IndexedDB
// under disk pressure or idle cleanup.
//
// Without `navigator.storage.persist()`, IDB is "best-effort" and can be
// wiped at the browser's discretion — catastrophic for Opake because the
// user's identity keypair lives in IDB. Without persisted storage, a single
// low-disk event can force full seed-phrase recovery.
//
// Browser behaviour reference:
// - Chrome: auto-grants if the origin has notification permission, is
//   installed as a PWA, or has high engagement. Otherwise may prompt or
//   silently refuse depending on version.
// - Firefox: auto-grants with engagement; otherwise prompts.
// - Safari: largely ignores and evicts after ~7 days of no interaction.
//
// The call is idempotent — we check `persisted()` first to avoid
// re-prompting. Result is memoized per session.

export type PersistenceOutcome =
  | { status: "persisted"; alreadyGranted: boolean }
  | { status: "denied" }
  | { status: "unsupported" };

// eslint-disable-next-line functional/no-let
let outcomePromise: Promise<PersistenceOutcome> | null = null;

/**
 * Request persistent storage from the browser. Memoized — subsequent calls
 * return the same outcome without re-querying.
 *
 * Call this before any IndexedDB write that holds sensitive state (identity
 * keys, session tokens). Safe to call multiple times.
 */
export function ensurePersistentStorage(): Promise<PersistenceOutcome> {
  outcomePromise ??= requestPersistence();
  return outcomePromise;
}

async function requestPersistence(): Promise<PersistenceOutcome> {
  // `typeof x === "function"` instead of truthy checks — lib.dom.d.ts
  // declares these as required, but Safari private mode and some older
  // browsers genuinely lack them at runtime. Direct property access
  // without optional chains to satisfy no-unnecessary-condition on the
  // always-defined `navigator.storage` parent.
  if (
    typeof navigator.storage.persist !== "function" ||
    typeof navigator.storage.persisted !== "function"
  ) {
    console.warn(
      "[storage] navigator.storage.persist() unavailable — IDB may be evicted without warning",
    );
    return { status: "unsupported" };
  }

  try {
    const alreadyGranted = await navigator.storage.persisted();
    if (alreadyGranted) {
      return { status: "persisted", alreadyGranted: true };
    }

    const granted = await navigator.storage.persist();
    if (granted) {
      console.info("[storage] persistent storage granted — IDB is now protected from eviction");
      return { status: "persisted", alreadyGranted: false };
    }

    console.warn(
      "[storage] persistent storage NOT granted — browser may evict IDB under disk pressure or idle cleanup",
    );
    return { status: "denied" };
  } catch (err) {
    // Some browser configurations throw here (e.g. private mode, restricted contexts).
    // Treat any throw as "unsupported" — the app must still function.
    console.warn("[storage] persist() threw, treating as unsupported:", err);
    return { status: "unsupported" };
  }
}

/**
 * Reset the memoized outcome. Test-only — resets the module-level promise
 * so tests can re-exercise the permission flow.
 */
export function __resetPersistenceCacheForTests(): void {
  outcomePromise = null;
}
