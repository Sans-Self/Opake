import { createRootRoute, Link, Outlet } from "@tanstack/react-router";
import {
  Files,
  ShareNetwork,
  SignIn,
  ShieldCheck,
} from "@phosphor-icons/react";

function RootLayout() {
  return (
    <div className="flex min-h-screen flex-col">
      <nav className="flex items-center gap-6 border-b border-neutral-200 px-6 py-3">
        <Link to="/" className="flex items-center gap-1.5 font-semibold">
          <ShieldCheck size={20} weight="bold" />
          Opake
        </Link>
        <div className="flex items-center gap-4">
          <Link
            to="/"
            className="flex items-center gap-1 text-sm [&.active]:font-medium"
          >
            <Files size={16} />
            Files
          </Link>
          <Link
            to="/shared"
            className="flex items-center gap-1 text-sm [&.active]:font-medium"
          >
            <ShareNetwork size={16} />
            Shared
          </Link>
          <Link
            to="/login"
            className="flex items-center gap-1 text-sm [&.active]:font-medium"
          >
            <SignIn size={16} />
            Login
          </Link>
        </div>
      </nav>
      <main className="flex-1 p-6">
        <Outlet />
      </main>
    </div>
  );
}

export const Route = createRootRoute({
  component: RootLayout,
});
