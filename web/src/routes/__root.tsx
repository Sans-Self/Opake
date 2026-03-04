import { createRootRoute, Outlet, useRouter } from "@tanstack/react-router";

function RootLayout() {
  return <Outlet />;
}

function RootError({ error }: { error: Error }) {
  const router = useRouter();

  return (
    <div className="flex min-h-screen items-center justify-center bg-base-300 font-sans">
      <div className="card card-bordered max-w-md bg-base-100 p-8 text-center">
        <h1 className="mb-2 text-lg font-medium text-error">
          Something went wrong
        </h1>
        <p className="mb-6 text-sm text-text-muted">{error.message}</p>
        <button
          onClick={() => router.invalidate()}
          className="btn btn-neutral btn-sm"
        >
          Try again
        </button>
      </div>
    </div>
  );
}

export const Route = createRootRoute({
  component: RootLayout,
  errorComponent: RootError,
});
