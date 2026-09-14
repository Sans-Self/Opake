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
      return {
        label: "Share retry",
        icon: ArrowsClockwiseIcon,
        detail: `${kind.retried} retried`,
      };
    case "memberWrapRepair":
      return {
        label: "Member access repair",
        icon: ShieldCheckIcon,
        detail: `${kind.repaired} repaired; ${kind.awaitingApproval} awaiting approval; ${kind.deferredHumanDecision + kind.deferredVisibility + kind.deferredByBudget} deferred`,
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

      {task.kind.type === "shareRetry" && task.kind.verificationErrors.map((issue) => (
        <p key={issue.uri} className="text-error text-caption mt-1">
          {issue.recipientDid}: published-key verification failed{issue.expired ? "; queued share expired" : ""}. {issue.reason}
        </p>
      ))}

      {task.kind.type === "shareRetry" && task.kind.completionNotices.map((notice) => (
        <VerificationNotice key={`${notice.did}-${notice.verification.state}`} notice={notice} />
      ))}

      {task.kind.type === "memberWrapRepair" && (
        <>
          {task.kind.verificationFailed > 0 && (
            <p className="text-error text-caption mt-1">
              {task.kind.verificationFailed} member key verification failure(s) need attention.
            </p>
          )}
          {task.kind.discoveryDeferred && (
            <p className="text-warning text-caption mt-1">Repair discovery deferred by this pass’s time budget.</p>
          )}
          {task.kind.verificationNotices.map((notice) => (
            <VerificationNotice key={`${notice.did}-${notice.verification.state}`} notice={notice} />
          ))}
        </>
      )}

      {typeof task.status === "object" && "failed" in task.status && (
        <p className="text-error text-caption mt-1">{task.status.failed}</p>
      )}

      <p className="text-caption text-text-faint mt-1">
        {new Date(task.updatedAt).toLocaleString()}
      </p>
    </li>
  );
}

function VerificationNotice({ notice }: { readonly notice: import("@opake/sdk").RecipientVerificationNotice }) {
  if (notice.verification.state === "unverified") {
    return <p className="text-caption text-warning mt-1">{notice.did}: unverified keys were explicitly approved.</p>;
  }
  if (notice.verification.anchorHistory === "replaced") {
    return <p className="text-caption text-warning mt-1">{notice.did}: DID verification method changed.</p>;
  }
  if (notice.verification.anchorHistory === "noHistory") {
    return <p className="text-caption text-warning mt-1">{notice.did}: DID method publishes no verification history.</p>;
  }
  if (notice.verification.anchorHistory === "unavailable") {
    return (
      <p className="text-caption text-warning mt-1">
        {notice.did}: verification history could not be read, so a replacement cannot be ruled out.
      </p>
    );
  }
  return null;
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
