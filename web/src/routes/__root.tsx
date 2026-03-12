import {
  createRootRouteWithContext,
  HeadContent,
  Link,
  Outlet,
  Scripts,
  useRouter,
} from "@tanstack/react-router";
import type { AuthSnapshot } from "@/stores/auth";
import { ToastContainer } from "@/components/ToastContainer";
import { OpakeLogo } from "@/components/OpakeLogo";
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
        <link
          rel="preload"
          href="/fonts/inter-latin-normal.woff2"
          as="font"
          type="font/woff2"
          crossOrigin=""
        />
        <link
          rel="preload"
          href="/fonts/cormorant-garamond-latin-normal.woff2"
          as="font"
          type="font/woff2"
          crossOrigin=""
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

function NotFound() {
  return (
    <div className="bg-base-300 flex min-h-screen flex-col items-center justify-center gap-6 px-6 font-sans">
      <OpakeLogo />
      <div className="text-center">
        <h1 className="font-display text-base-content mb-2 text-[clamp(2rem,5vw,3.4rem)] font-normal tracking-tight">
          Page not found
        </h1>
        <p className="text-text-muted text-[0.95rem]">
          The page you&rsquo;re looking for doesn&rsquo;t exist, or it moved.
        </p>
      </div>
      <div className="flex gap-3">
        <Link to="/" className="btn btn-neutral btn-sm">
          Back to home
        </Link>
        <Link to="/docs" className="btn btn-outline border-border-accent text-secondary btn-sm">
          Read the docs
        </Link>
      </div>
    </div>
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
  notFoundComponent: NotFound,
});
