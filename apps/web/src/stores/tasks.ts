// Daemon task store — in-memory TaskStore impl + Zustand reactive layer.
//
// Tasks are ephemeral UI state — the daemon re-runs them on reload.
// The store bridges @opake/daemon's generic TaskRecord to the typed
// DaemonTask discriminated union that tasks.lazy.tsx renders.

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import type { TaskRecord, TaskStore } from "@opake/daemon";

// ---------------------------------------------------------------------------
// Public types (consumed by tasks.lazy.tsx)
// ---------------------------------------------------------------------------

export type DaemonTaskKind =
  | { readonly type: "pairCleanup"; readonly deleted: number }
  | { readonly type: "grantHealing"; readonly healed: number }
  | { readonly type: "shareRetry"; readonly retried: number }
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

function mapKind(raw: Readonly<Record<string, unknown>>): DaemonTaskKind {
  const type = raw.type;
  switch (type) {
    case "pairCleanup":
      return { type: "pairCleanup", deleted: typeof raw.deleted === "number" ? raw.deleted : 0 };
    case "grantHealing":
      return { type: "grantHealing", healed: typeof raw.healed === "number" ? raw.healed : 0 };
    case "shareRetry":
      return { type: "shareRetry", retried: typeof raw.retried === "number" ? raw.retried : 0 };
    default:
      return { type: "unknown", raw };
  }
}

function recordToTask(record: TaskRecord): DaemonTask {
  return {
    id: record.id,
    kind: mapKind(record.kind),
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
