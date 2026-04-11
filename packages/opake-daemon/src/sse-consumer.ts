// SSE-driven proposal sync. Reacts to appview events instead of polling,
// debouncing per-keyring so rapid proposal bursts collapse into one sync.
// Events from our own DID within a short window after a local write are
// suppressed to avoid redundant reloads.

import type { Opake, EventStream, SSEKeyring } from "@opake/sdk";
import type { DaemonOptions, SSEConfig, TaskStore } from "./types";
import { persistSyncResult, handleDaemonError } from "./tasks";

const PROPOSAL_DEBOUNCE_MS = 2_000;
const RECORD_CHANGED_DEBOUNCE_MS = 500;
const SELF_EVENT_SUPPRESS_MS = 3_000;

/** Shape common to all record events — what we need for self-event filtering. */
interface RecordEvent {
  readonly owner_did: string;
}

/** Shape common to all proposal events — what we need for routing. */
interface ProposalEvent {
  readonly author_did: string;
  readonly keyring_uri?: string | null;
}

export interface SSEConsumerHandle {
  close(): void;
}

/**
 * Subscribe to SSE and sync workspaces when proposal events arrive.
 *
 * Record events fire `sse.onRecordChanged` (unless suppressed as self-events).
 * Proposal events trigger debounced per-workspace sync, which fires
 * `options.onWorkspaceUpdated` on proposal application.
 */
export function startSSEConsumer(
  opake: Opake,
  taskStore: TaskStore,
  options: DaemonOptions,
  sse: SSEConfig,
): SSEConsumerHandle {
  const proposalTimers = new Map<string, ReturnType<typeof setTimeout>>();
  // Reserved key for full-sync debounce — cannot collide with a real AT URI
  // since those always start with "at://".
  const FULL_SYNC_KEY = "__full__";

  // eslint-disable-next-line functional/no-let -- mutable ref for cleanup
  let stream: EventStream | null = null;
  // eslint-disable-next-line functional/no-let -- mutable ref for debounce
  let recordChangedTimer: ReturnType<typeof setTimeout> | null = null;

  // DID resolved once at startup. The daemon is torn down on account switch,
  // so staleness isn't a concern.
  const selfDid = opake.getDid();

  function isSelfEvent(did: string | null | undefined): boolean {
    if (!selfDid || !did || did !== selfDid) return false;
    return (Date.now() - opake.lastWriteAt) < SELF_EVENT_SUPPRESS_MS;
  }

  /** Leading-edge debounce: schedule one reload per burst window. */
  function notifyRecordChanged(did?: string | null): void {
    if (isSelfEvent(did)) return;
    if (recordChangedTimer) return;
    recordChangedTimer = setTimeout(() => {
      recordChangedTimer = null;
      sse.onRecordChanged?.();
    }, RECORD_CHANGED_DEBOUNCE_MS);
  }

  /** Trailing-edge debounce per keyring. Rapid bursts collapse into one sync. */
  function debouncedSync(keyringUri: string | null): void {
    const key = keyringUri ?? FULL_SYNC_KEY;
    const existing = proposalTimers.get(key);
    if (existing) clearTimeout(existing);

    proposalTimers.set(
      key,
      setTimeout(() => {
        proposalTimers.delete(key);
        void (keyringUri ? syncSingleWorkspace(keyringUri) : syncAllWorkspaces());
      }, PROPOSAL_DEBOUNCE_MS),
    );
  }

  async function syncSingleWorkspace(keyringUri: string): Promise<void> {
    try {
      const result = await opake.syncWorkspaceByUri(keyringUri);
      if (result && (result.proposalsApplied > 0 || result.error)) {
        await persistSyncResult(taskStore, result);
        if (result.proposalsApplied > 0) {
          options.onWorkspaceUpdated?.([keyringUri]);
        }
      }
    } catch (err) {
      handleDaemonError(err, options);
    }
  }

  async function syncAllWorkspaces(): Promise<void> {
    try {
      const results = await opake.syncOwnedWorkspacesDetailed();
      const updated = results.filter((r) => r.proposalsApplied > 0);
      if (updated.length > 0) {
        await Promise.all(updated.map((r) => persistSyncResult(taskStore, r)));
        options.onWorkspaceUpdated?.(updated.map((r) => r.keyringUri));
      }
    } catch (err) {
      handleDaemonError(err, options);
    }
  }

  // Generic handlers — all record events share the same "notify on non-self"
  // behavior; all proposal events share the same self-filter + per-keyring debounce.

  const handleRecordEvent = (data: RecordEvent): void => notifyRecordChanged(data.owner_did);
  const handleDelete = (): void => notifyRecordChanged();

  function handleProposal(data: ProposalEvent): void {
    if (isSelfEvent(data.author_did)) return;
    debouncedSync(data.keyring_uri ?? null);
  }

  function handleKeyringUpsert(data: SSEKeyring): void {
    if (isSelfEvent(data.owner_did) || !data.uri) return;
    // Two paths: direct mutation (add member, rename) → sidebar needs refresh
    // immediately; proposal application → debouncedSync fires onWorkspaceUpdated
    // again later. Both call onWorkspaceUpdated. The host app's loadWorkspaces
    // dedupes in-flight calls and skips no-op updates, so the duplicate is free.
    options.onWorkspaceUpdated?.([data.uri]);
    debouncedSync(data.uri);
  }

  try {
    stream = opake.subscribe(
      {
        onDirectoryUpsert: handleRecordEvent,
        onDirectoryDelete: handleDelete,
        onDocumentUpsert: handleRecordEvent,
        onDocumentDelete: handleDelete,
        onKeyringUpsert: handleKeyringUpsert,
        onKeyringDelete: handleDelete,
        onGrantUpsert: handleRecordEvent,
        onGrantDelete: handleDelete,
        onDirectoryUpdateUpsert: handleProposal,
        onDirectoryUpdateDelete: handleDelete,
        onKeyringUpdateUpsert: handleProposal,
        onKeyringUpdateDelete: handleDelete,
        onDocumentUpdateUpsert: handleProposal,
        onDocumentUpdateDelete: handleDelete,
        onReconnect: () => {
          // syncAllWorkspaces fires onWorkspaceUpdated, which the host app
          // routes back to reloadCurrentDirectory. No separate notify needed.
          void syncAllWorkspaces();
        },
      },
      sse.appviewUrl,
    );
  } catch {
    // SSE connection failure is non-fatal — timer fallback still runs
  }

  function close(): void {
    stream?.close();
    stream = null;
    if (recordChangedTimer) {
      clearTimeout(recordChangedTimer);
      recordChangedTimer = null;
    }
    for (const timer of proposalTimers.values()) {
      clearTimeout(timer);
    }
    proposalTimers.clear();
  }

  return { close };
}
