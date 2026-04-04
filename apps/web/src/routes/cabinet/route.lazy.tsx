import { createLazyFileRoute, Outlet, Link } from "@tanstack/react-router";
import { OpakeLogo } from "@/components/OpakeLogo";
import { useAppStore } from "@/stores/app";

function CabinetLayout() {
  const anyLoading = useAppStore((s) => s.anythingLoading());

  return (
    <div className="bg-base-300 flex h-screen overflow-hidden font-sans">
      <aside className="bg-base-200 border-border-accent/30 hidden w-53 flex-col border-r md:flex">
        <div className="flex items-center gap-2 px-4 py-4">
          <Link to="/">
            <OpakeLogo size="sm" loading={anyLoading} />
          </Link>
        </div>
        <nav className="flex-1 px-2 py-2">
          <p className="text-base-content/40 px-2 text-xs">Cabinet routes not yet wired to SDK</p>
        </nav>
      </aside>
      <main className="flex flex-1 flex-col overflow-hidden">
        <Outlet />
      </main>
    </div>
  );
}

export const Route = createLazyFileRoute("/cabinet")({
  component: CabinetLayout,
});
