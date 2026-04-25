// Shared dedup guard for keeper bootstraps.
//
// Used by `useWorkspaces` and `useInbox` to dedupe the initial
// `opake.list*()` call across concurrent mounts (e.g. StrictMode
// double-mount, or multiple components consuming the same hook).
//
// Keyed by (Opake instance, label):
//   - Keying on the Opake instance isolates accounts — switching from
//     account A to account B constructs a fresh Opake, so B's bootstrap
//     isn't short-circuited by a stale A-era in-flight promise.
//   - Keying on a string label lets a single Opake run multiple
//     independent bootstraps (workspaces, inbox, ...) without them
//     deduplicating against each other.
//
// Uses a WeakMap so in-flight entries clear naturally when an Opake
// instance is GC'd; the inner Map also deletes each label on settle.

import type { Opake } from "@opake/sdk";

const inFlight = new WeakMap<Opake, Map<string, Promise<unknown>>>();

/**
 * Run a bootstrap once per (Opake, label) pair. If a prior call with
 * the same Opake + label is still pending, this is a no-op.
 *
 * @param opake - Opake instance identifying the account scope.
 * @param label - Human-readable name used for log context and dedup.
 * @param fetch - Returns the bootstrap promise; called at most once per pair.
 */
export function bootstrapOnce(
  opake: Opake,
  label: string,
  fetch: () => Promise<unknown>,
): void {
  // eslint-disable-next-line functional/no-let -- need mutable slot for lazy-init
  let labels = inFlight.get(opake);
  if (labels?.has(label)) return;
  if (!labels) {
    labels = new Map();
    inFlight.set(opake, labels);
  }
  const slot = labels;
  const promise = fetch()
    .catch((err: unknown) => {
      console.warn(`[opake-react] ${label} bootstrap failed:`, err);
    })
    .finally(() => {
      slot.delete(label);
    });
  slot.set(label, promise);
}
