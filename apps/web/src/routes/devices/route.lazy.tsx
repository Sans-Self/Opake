import { createLazyFileRoute, Link, Outlet, useRouterState } from "@tanstack/react-router";
import { OpakeLogo } from "@/components/OpakeLogo";
import { useAppStore } from "@/stores/app";
import { useAuthStore } from "@/stores/auth";
import { ArrowLeftIcon, WarningIcon } from "@phosphor-icons/react";

function View() {
  const { anythingLoading } = useAppStore();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const sessionActive = useAuthStore((s) => s.session.status === "active");
  const showCabinetReturn = sessionActive && pathname.startsWith("/devices/pair/");

  return (
    <div className="bg-base-300 relative flex min-h-screen justify-center font-sans sm:pt-32">
      {showCabinetReturn ? (
        <Link
          to="/cabinet/files"
          className="text-base-content/70 hover:text-base-content absolute top-4 left-4 flex items-center gap-1.5 text-sm transition-colors sm:top-6 sm:left-6"
        >
          <ArrowLeftIcon size={14} weight="bold" />
          <span>Back to my cabinet</span>
        </Link>
      ) : null}
      <div className="flex w-full flex-col items-center gap-8 px-6 py-12">
        <OpakeLogo size="xl" loading={anythingLoading()} />
        <Outlet />
      </div>
    </div>
  );
}

function ErrorView() {
  return (
    <div className="flex h-screen flex-col items-center gap-6 pt-32 text-center">
      <div>
        <OpakeLogo size="xl" />
      </div>
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

export const Route = createLazyFileRoute("/devices")({
  component: View,
  errorComponent: ErrorView,
});
