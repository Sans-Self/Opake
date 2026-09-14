// Daemon task store — in-memory TaskStore impl + Zustand reactive layer.
//
// Tasks are ephemeral UI state — the daemon re-runs them on reload.
// The store bridges @opake/daemon's generic TaskRecord to the typed
// DaemonTask discriminated union that tasks.lazy.tsx renders.

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import type { TaskRecord, TaskStore } from "@opake/daemon";
import type {
  AnchorHistory,
  PendingShareVerificationError,
  RecipientVerificationNotice,
} from "@opake/sdk";

const ANCHOR_HISTORIES: readonly AnchorHistory[] = [
  "notReplaced",
  "replaced",
  "noHistory",
  "unavailable",
];

function isAnchorHistory(value: unknown): value is AnchorHistory {
  return ANCHOR_HISTORIES.some((history) => history === value);
}

// ---------------------------------------------------------------------------
// Public types (consumed by tasks.lazy.tsx)
// ---------------------------------------------------------------------------

export type DaemonTaskKind =
  | { readonly type: "pairCleanup"; readonly deleted: number }
  | { readonly type: "grantHealing"; readonly healed: number }
  | {
      readonly type: "shareRetry";
      readonly retried: number;
      readonly verificationErrors: PendingShareVerificationError[];
      readonly completionNotices: RecipientVerificationNotice[];
    }
  | {
      readonly type: "memberWrapRepair";
      readonly repaired: number;
      readonly verificationNotices: RecipientVerificationNotice[];
      readonly awaitingApproval: number;
      readonly verificationFailed: number;
      readonly deferredHumanDecision: number;
      readonly deferredVisibility: number;
      readonly deferredByBudget: number;
      readonly discoveryDeferred: boolean;
    }
  | { readonly type: "unknown"; readonly raw: Readonly<Record<string, unknown>> };

export type TaskStatus = "running" | "completed" | { readonly failed: string };

export interface DaemonTask {
  readonly id: string;
  readonly kind: DaemonTaskKind;
  readonly status: TaskStatus;
  readonly createdAt: string;
  readonly updatedAt: string;
}

// ---------------------------------------------------------------------------
// TaskRecord → DaemonTask mapping (validated, not cast)
// ---------------------------------------------------------------------------

export function mapTaskKind(raw: Readonly<Record<string, unknown>>): DaemonTaskKind {
  const type = raw.type;
  switch (type) {
    case "pairCleanup":
      return { type: "pairCleanup", deleted: typeof raw.deleted === "number" ? raw.deleted : 0 };
    case "grantHealing":
      return { type: "grantHealing", healed: typeof raw.healed === "number" ? raw.healed : 0 };
    case "shareRetry":
      return {
        type: "shareRetry",
        retried: typeof raw.retried === "number" ? raw.retried : 0,
        verificationErrors: Array.isArray(raw.verificationErrors)
          ? raw.verificationErrors.flatMap((issue): readonly PendingShareVerificationError[] => {
              if (issue === null || typeof issue !== "object" || Array.isArray(issue)) return [];
              const value = issue as Record<string, unknown>;
              if (
                typeof value.uri !== "string"
                || typeof value.recipientDid !== "string"
                || typeof value.reason !== "string"
                || typeof value.expired !== "boolean"
              ) return [];
              return [{
                uri: value.uri,
                recipientDid: value.recipientDid,
                reason: value.reason,
                expired: value.expired,
              }];
            })
          : [],
        completionNotices: verificationNotices(raw.completionNotices),
      };
    case "memberWrapRepair":
      return {
        type: "memberWrapRepair",
        repaired: numberField(raw.repaired),
        verificationNotices: verificationNotices(raw.verificationNotices),
        awaitingApproval: numberField(raw.awaitingApproval),
        verificationFailed: numberField(raw.verificationFailed),
        deferredHumanDecision: numberField(raw.deferredHumanDecision),
        deferredVisibility: numberField(raw.deferredVisibility),
        deferredByBudget: numberField(raw.deferredByBudget),
        discoveryDeferred: raw.discoveryDeferred === true,
      };
    default:
      return { type: "unknown", raw };
  }
}

function numberField(value: unknown): number {
  return typeof value === "number" ? value : 0;
}

function verificationNotices(value: unknown): RecipientVerificationNotice[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((notice): readonly RecipientVerificationNotice[] => {
    if (notice === null || typeof notice !== "object" || Array.isArray(notice)) return [];
    const item = notice as Record<string, unknown>;
    const verification = item.verification;
    if (typeof item.did !== "string" || verification === null || typeof verification !== "object") return [];
    const state = verification as Record<string, unknown>;
    if (state.state === "unverified") return [{ did: item.did, verification: { state: "unverified" } }];
    if (state.state === "verified" && isAnchorHistory(state.anchorHistory)) {
      return [{ did: item.did, verification: { state: "verified", anchorHistory: state.anchorHistory } }];
    }
    return [];
  });
}

function recordToTask(record: TaskRecord): DaemonTask {
  return {
    id: record.id,
    kind: mapTaskKind(record.kind),
    status: record.status,
    createdAt: record.createdAt,
    updatedAt: record.updatedAt,
  };
}

// ---------------------------------------------------------------------------
// In-memory TaskStore implementation
//
// The TaskStore interface from @opake/daemon requires async methods (for
// IndexedDB compatibility). Our in-memory implementation wraps sync Map
// operations in Promise.resolve().
// ---------------------------------------------------------------------------

const taskMap = new Map<string, TaskRecord>();

/* eslint-disable functional/immutable-data -- TaskStore is a mutable data store by design */
export const taskStore: TaskStore = {
  saveTask: (task: TaskRecord): Promise<void> => {
    taskMap.set(task.id, task);
    syncToZustand();
    return Promise.resolve();
  },

  loadTasks: (): Promise<readonly TaskRecord[]> => {
    return Promise.resolve([...taskMap.values()]);
  },

  deleteTask: (id: string): Promise<void> => {
    taskMap.delete(id);
    syncToZustand();
    return Promise.resolve();
  },
};
/* eslint-enable functional/immutable-data */

// ---------------------------------------------------------------------------
// Zustand store (reactive layer for React)
// ---------------------------------------------------------------------------

interface TasksState {
  tasks: readonly DaemonTask[];
  loaded: boolean;
}

interface TasksActions {
  loadTasks(): Promise<void>;
}

type TasksStore = TasksState & TasksActions;

export const useTaskStore = create<TasksStore>()(
  immer((set) => ({
    tasks: [],
    loaded: false,

    async loadTasks() {
      const records = await taskStore.loadTasks();
      set((draft) => {
        draft.tasks = records.map(recordToTask);
        draft.loaded = true;
      });
    },
  })),
);

/** Push Map state into Zustand — called after every taskStore mutation. */
function syncToZustand(): void {
  const records = [...taskMap.values()];
  useTaskStore.setState((draft) => {
    draft.tasks = records.map(recordToTask);
    draft.loaded = true;
  });
}
