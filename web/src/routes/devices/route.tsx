import { OpakeLogo } from "@/components/OpakeLogo";
import { useAppStore } from "@/stores/app";
import { useAuthStore } from "@/stores/auth";
import { useMinimumDuration } from "@/utils";
import { WarningIcon } from "@phosphor-icons/react";
import { createFileRoute, Outlet, useMatchRoute } from "@tanstack/react-router";

const MIN_CHECK_DISPLAY_MS = 2000;

export const Route = createFileRoute("/devices")({
  ssr: false,
  component: View,
  errorComponent: ErrorView,
});

function View() {
  const { anythingLoading } = useAppStore();
  const matchRoute = useMatchRoute();
  const identity = useAuthStore((s) => s.identity);
  const minTimeElapsed = useMinimumDuration(MIN_CHECK_DISPLAY_MS);
  const isIndex = !!matchRoute({ to: "/devices" });
  const isChecking = identity.status === "unchecked" || identity.status === "checking";
  const showLogo = !isIndex || (!isChecking && minTimeElapsed);

  return (
    <div className="bg-base-300 flex min-h-screen justify-center font-sans sm:pt-32">
      <div className="flex w-full flex-col items-center gap-8 px-6 py-12">
        {showLogo && <OpakeLogo size="xl" loading={anythingLoading()} />}
        <Outlet />
      </div>
    </div>
  );
}

function ErrorView() {
  return (
    <div className="flex flex-col items-center gap-6 text-center">
      <WarningIcon size={48} className="text-error" weight="fill" />
      <div className="flex flex-col gap-2">
        <h1 className="text-base-content text-2xl font-semibold">Something went wrong</h1>
      </div>
      <a href="/devices" className="btn btn-neutral btn-sm">
        Back to devices
      </a>
    </div>
  );
}
