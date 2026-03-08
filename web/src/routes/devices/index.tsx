import { useEffect } from "react";
import { createFileRoute, redirect, useMatchRoute } from "@tanstack/react-router";
import { useAuthStore } from "@/stores/auth";
import { WarningIcon } from "@phosphor-icons/react";
import { PageHeader } from "@/components/devices/PageHeader";
import { CheckingView } from "@/components/devices/CheckingView";
import { FreshAccountView } from "@/components/devices/FreshAccountView";
import { RecoverIdentityView } from "@/components/devices/RecoverIdentityView";
import { ConflictView } from "@/components/devices/ConflictView";
import { ReadyView } from "@/components/devices/ReadyView";
import { useMinimumDuration } from "@/utils";

const MIN_CHECK_DISPLAY_MS = 2000;

export const Route = createFileRoute("/devices/")({
  beforeLoad: ({ context }) => {
    if (context.auth.session.status !== "active") {
      throw redirect({ to: "/devices/login" });
    }
  },
  component: DevicesPage,
  errorComponent: ErrorView,
});

function DevicesPage() {
  const identity = useAuthStore((s) => s.identity);
  const minTimeElapsed = useMinimumDuration(MIN_CHECK_DISPLAY_MS);
  const matchRoute = useMatchRoute();

  const childRouteActive =
    !!matchRoute({ to: "/devices/pair/request", fuzzy: true }) ||
    !!matchRoute({ to: "/devices/pair/accept", fuzzy: true });

  useEffect(() => {
    if (childRouteActive) return;
    if (identity.status !== "unchecked") return;
    void useAuthStore.getState().checkIdentity();
  }, [identity.status, childRouteActive]);

  if (childRouteActive) return null;

  const isChecking = identity.status === "unchecked" || identity.status === "checking";

  if (isChecking || !minTimeElapsed) {
    return <CheckingView />;
  }

  switch (identity.status) {
    case "fresh":
      return <FreshAccountView />;
    case "remote_only":
      return <RecoverIdentityView />;
    case "conflict":
      return <ConflictView />;
    case "ready":
      return <ReadyView />;
  }
}

function ErrorView() {
  return (
    <div className="flex flex-col items-center gap-6 text-center">
      <PageHeader icon={WarningIcon} iconClassName="text-error" title="Something went wrong" />
      <a href="/devices" className="btn btn-neutral btn-sm">
        Back to devices
      </a>
    </div>
  );
}
