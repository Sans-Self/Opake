import { useState } from "react";
import { createLazyFileRoute } from "@tanstack/react-router";
import { useAuthStore } from "@/stores/auth";

// Curated list of Atmosphere apps, rendered under the sign-in form. Each
// entry gives visitors a concrete starting point if they don't yet have an
// atproto account — and signals to existing users that their handle from
// any of these places already works here.
const ATMOSPHERE_APPS: ReadonlyArray<{
  readonly name: string;
  readonly href: string;
  readonly description: string;
}> = [
  {
    name: "Bluesky",
    href: "https://bsky.app",
    description: "The microblogging app most people start with. Millions of handles.",
  },
  {
    name: "Eurosky",
    href: "https://eurosky.social",
    description: "EU-hosted community and infrastructure — data stays in Europe.",
  },
  {
    name: "pckt.blog",
    href: "https://pckt.blog",
    description: "Longform blogging — posts live as atproto records.",
  },
  {
    name: "Tangled",
    href: "https://tangled.sh",
    description: "A git forge that signs you in with your atproto handle.",
  },
  {
    name: "Smoke Signal",
    href: "https://smokesignal.events",
    description: "Event planning with RSVPs, all on the open network.",
  },
];

function LoginPage() {
  const session = useAuthStore((s) => s.session);
  const startLogin = useAuthStore((s) => s.startLogin);
  const [handle, setHandle] = useState("");
  const isLoading = session.status === "authenticating";
  const errorMessage = session.status === "error" ? session.message : null;

  const handleSubmit = (e: React.SyntheticEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!handle.trim()) return;
    void startLogin(handle.trim());
  };

  return (
    <div className="mx-auto flex w-full max-w-lg flex-col gap-6 px-4 py-8">
      <form onSubmit={handleSubmit} className="card card-bordered bg-base-100 w-full p-6">
        <h1 className="text-ui text-base-content mb-1 font-medium">
          Sign in with your Atmosphere account
        </h1>
        <p className="text-caption text-text-muted mb-5 leading-relaxed">
          Use the handle from any AT Protocol app — Bluesky, your own PDS, or anywhere else in the
          Atmosphere. Opake doesn't issue its own accounts.
        </p>
        <label className="input input-bordered mb-3 flex w-full items-center gap-2">
          <input
            type="text"
            placeholder="you.bsky.social"
            value={handle}
            onChange={(e) => setHandle(e.target.value)}
            className="grow"
            required
            disabled={isLoading}
            aria-label="Your Atmosphere handle"
          />
        </label>
        {errorMessage && (
          <p className="text-caption text-error mb-3" role="alert">
            {errorMessage}
          </p>
        )}
        <button type="submit" className="btn btn-neutral w-full" disabled={isLoading}>
          {isLoading ? (
            <span className="loading loading-spinner loading-sm" />
          ) : (
            "Continue to your PDS"
          )}
        </button>
        <p className="text-caption text-text-faint mt-3 leading-relaxed">
          You'll be redirected to your PDS (the provider that holds your account) to authorize
          Opake. Your password never reaches us.
        </p>
      </form>

      <section
        aria-labelledby="atmosphere-heading"
        className="card card-bordered bg-base-100/60 w-full p-5"
      >
        <h2
          id="atmosphere-heading"
          className="text-ui text-base-content mb-1 font-medium"
        >
          What's an Atmosphere account?
        </h2>
        <p className="text-caption text-text-muted mb-4 leading-relaxed">
          The Atmosphere is the open network of apps built on the AT Protocol. Your account lives
          with one provider — your PDS — and works across every app on the network. One handle,
          many apps, no duplicate signups. If you already use any of these, you're already set:
        </p>
        <ul className="flex flex-col gap-2">
          {ATMOSPHERE_APPS.map((app) => (
            <li key={app.name}>
              <a
                href={app.href}
                target="_blank"
                rel="noopener noreferrer"
                className="border-border-accent/40 bg-base-100 hover:border-primary/60 hover:bg-accent/30 block rounded-lg border p-2.5 transition-colors"
              >
                <div className="text-ui text-base-content font-medium">{app.name}</div>
                <div className="text-caption text-text-muted leading-relaxed">
                  {app.description}
                </div>
              </a>
            </li>
          ))}
        </ul>
        <p className="text-caption text-text-faint mt-4 leading-relaxed">
          Prefer self-hosting? Run your own PDS and use the handle on it here — same flow, fully
          independent of any provider.
        </p>
      </section>
    </div>
  );
}

export const Route = createLazyFileRoute("/devices/login")({
  component: LoginPage,
});
