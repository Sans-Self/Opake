import { useEffect } from "react";
import { createLazyFileRoute } from "@tanstack/react-router";
import { ArrowsClockwiseIcon, ShieldCheckIcon } from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { useTaskStore, type DaemonTask, type TaskStatus } from "@/stores/tasks";

function TasksPage() {
  const tasks = useTaskStore((s) => s.tasks);
  const loaded = useTaskStore((s) => s.loaded);
  const loadTasks = useTaskStore((s) => s.loadTasks);

  useEffect(() => {
    void loadTasks();
    const interval = setInterval(() => void loadTasks(), 10_000);
    return () => clearInterval(interval);
  }, [loadTasks]);

  return (
    <PanelShell
      depth={0}
      breadcrumbs={<span className="text-body font-medium">Background Tasks</span>}
      footer="Daemon-managed background tasks"
    >
      {!loaded ? (
        <p className="text-text-faint text-body p-4">Loading...</p>
      ) : tasks.length === 0 ? (
        <div className="text-text-faint text-body flex flex-col items-center gap-2 py-16">
          <ArrowsClockwiseIcon size={32} weight="thin" />
          <p>No background tasks.</p>
        </div>
      ) : (
        <ul className="flex flex-col gap-2 p-4">
          {tasks.map((task) => (
            <TaskCard key={task.id} task={task} />
          ))}
        </ul>
      )}
    </PanelShell>
  );
}

function kindMeta(kind: DaemonTask["kind"]): {
  label: string;
  icon: typeof ShieldCheckIcon;
  detail?: string;
} {
  switch (kind.type) {
    case "pairCleanup":
      return {
        label: "Pair cleanup",
        icon: ArrowsClockwiseIcon,
        detail: `${kind.deleted} deleted`,
      };
    case "grantHealing":
      return { label: "Grant healing", icon: ShieldCheckIcon, detail: `${kind.healed} healed` };
    case "shareRetry":
      return { label: "Share retry", icon: ArrowsClockwiseIcon, detail: `${kind.retried} retried` };
    case "proposalSync":
      return {
        label: "Proposal sync",
        icon: ArrowsClockwiseIcon,
        detail: `${kind.proposalsApplied} applied`,
      };
    case "unknown":
      return { label: "Background task", icon: ArrowsClockwiseIcon };
  }
}

function TaskCard({ task }: { readonly task: DaemonTask }) {
  const { label: kindLabel, icon: KindIcon, detail: kindDetail } = kindMeta(task.kind);
  const status = statusLabel(task.status);
  const statusColor = statusColorClass(task.status);

  return (
    <li className="bg-base-100 border-base-300/50 rounded-lg border p-3">
      <div className="flex items-center gap-2">
        <KindIcon size={18} className="text-text-muted shrink-0" />
        <span className="text-body flex-1 font-medium">{kindLabel}</span>
        <span className={`badge badge-sm ${statusColor}`}>{status}</span>
      </div>

      {kindDetail && <p className="text-caption text-text-faint mt-1">{kindDetail}</p>}

      {typeof task.status === "object" && "failed" in task.status && (
        <p className="text-error text-caption mt-1">{task.status.failed}</p>
      )}

      <p className="text-caption text-text-faint mt-1">
        {new Date(task.updatedAt).toLocaleString()}
      </p>
    </li>
  );
}

function statusLabel(s: TaskStatus): string {
  if (s === "running") return "Running";
  if (s === "completed") return "Completed";
  return "Failed";
}

function statusColorClass(s: TaskStatus): string {
  if (s === "running") return "badge-primary";
  if (s === "completed") return "badge-success";
  return "badge-error";
}

export const Route = createLazyFileRoute("/cabinet/tasks")({
  component: TasksPage,
});
