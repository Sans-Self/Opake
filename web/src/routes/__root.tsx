import { createRootRouteWithContext, Outlet, useRouter } from "@tanstack/react-router";
import type { AuthSnapshot } from "@/stores/auth";

export interface RouterContext {
  auth: AuthSnapshot;
}

function RootLayout() {
  return <Outlet />;
}

function RootError({ error }: Readonly<{ error: Error }>) {
  const router = useRouter();

  return (
    <div className="bg-base-300 flex min-h-screen items-center justify-center font-sans">
      <div className="card card-bordered bg-base-100 max-w-md p-8 text-center">
        <h1 className="text-error mb-2 text-lg font-medium">Something went wrong</h1>
        <p className="text-text-muted mb-6 text-sm">{error.message}</p>
        <button
          onClick={() => {
            void router.invalidate();
          }}
          className="btn btn-neutral btn-sm"
        >
          Try again
        </button>
      </div>
    </div>
  );
}

export const Route = createRootRouteWithContext<RouterContext>()({
  component: RootLayout,
  errorComponent: RootError,
});
