import {
  createRootRouteWithContext,
  HeadContent,
  Outlet,
  Scripts,
  useRouter,
} from "@tanstack/react-router";
import type { AuthSnapshot } from "@/stores/auth";
import { ToastContainer } from "@/components/ToastContainer";
import css from "@/index.css?url";

export interface RouterContext {
  auth: AuthSnapshot;
}

function RootDocument({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" data-theme="opake">
      <head>
        <meta charSet="UTF-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <link rel="preconnect" href="https://fonts.googleapis.com" />
        <link rel="preconnect" href="https://fonts.gstatic.com" crossOrigin="" />
        <link
          href="https://fonts.googleapis.com/css2?family=Cormorant+Garamond:ital,wght@0,300;0,400;0,500;0,600;1,300;1,400;1,500;1,600&family=Inter:ital,opsz,wght@0,14..32,300;0,14..32,400;0,14..32,500;0,14..32,600;1,14..32,300;1,14..32,400&display=swap"
          rel="stylesheet"
        />
        <link rel="stylesheet" href={css} />
        <HeadContent />
      </head>
      <body>
        {children}
        <Scripts />
      </body>
    </html>
  );
}

function RootLayout() {
  return (
    <RootDocument>
      <Outlet />
      <ToastContainer />
    </RootDocument>
  );
}

function RootError({ error }: Readonly<{ error: Error }>) {
  const router = useRouter();

  return (
    <RootDocument>
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
    </RootDocument>
  );
}

export const Route = createRootRouteWithContext<RouterContext>()({
  head: () => ({
    meta: [
      { title: "Opake" },
      { name: "og:site_name", content: "Opake" },
      { name: "og:type", content: "website" },
      { name: "twitter:card", content: "summary" },
    ],
  }),
  component: RootLayout,
  errorComponent: RootError,
});
