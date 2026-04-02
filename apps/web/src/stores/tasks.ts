// Task store — daemon background task visibility.
// Tasks are persisted in IndexedDB and displayed on the /cabinet/tasks route.

import { create } from "zustand";
import { storage } from "@/lib/indexeddbStorage";

/** Matches opake-core DaemonTask shape (camelCase serde). */
export interface DaemonTask {
  readonly id: string;
  readonly kind: DaemonTaskKind;
  readonly status: TaskStatus;
  readonly progress: TaskProgress | null;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export type DaemonTaskKind =
  | SessionRefreshKind
  | PairCleanupKind
  | GrantHealingKind
  | ShareRetryKind
  | ProposalSyncKind
  | ReEncryptionKind;

interface SessionRefreshKind {
  readonly type: "sessionRefresh";
}

interface PairCleanupKind {
  readonly type: "pairCleanup";
  readonly deleted: number;
}

interface GrantHealingKind {
  readonly type: "grantHealing";
  readonly healed: number;
}

interface ShareRetryKind {
  readonly type: "shareRetry";
  readonly retried: number;
}

interface ProposalSyncKind {
  readonly type: "proposalSync";
  readonly keyringUri: string;
  readonly proposalsApplied: number;
}

interface ReEncryptionKind {
  readonly type: "reEncryption";
  readonly keyringUri: string;
  readonly fromRotation: number;
  readonly toRotation: number;
}

export type TaskStatus = "pending" | "running" | "completed" | { readonly failed: string };

export interface TaskProgress {
  readonly completed: number;
  readonly remaining: number;
  readonly bytesProcessed: number;
}

interface TaskState {
  readonly tasks: readonly DaemonTask[];
  readonly loaded: boolean;
  loadTasks: () => Promise<void>;
}

export const useTaskStore = create<TaskState>((set) => ({
  tasks: [],
  loaded: false,

  loadTasks: async () => {
    const raw = await storage.loadTasks();
    const tasks = raw as DaemonTask[];
    set({ tasks, loaded: true });
  },
}));
