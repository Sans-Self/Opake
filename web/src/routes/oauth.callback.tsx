import { useEffect, useRef } from "react";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { OpakeLogo } from "@/components/OpakeLogo";
import { useAuthStore } from "@/stores/auth";

function OAuthCallbackPage() {
  const navigate = useNavigate();
  const phase = useAuthStore((s) => s.phase);
  const completeLogin = useAuthStore((s) => s.completeLogin);
  const errorMessage = phase === "error" ? (useAuthStore.getState() as { message: string }).message : null;

  const hasStartedRef = useRef(false);

  useEffect(() => {
    if (hasStartedRef.current) return;
    hasStartedRef.current = true;

    const params = new URLSearchParams(window.location.search);
    const code = params.get("code");
    const state = params.get("state");

    // Strip query params from URL immediately
    window.history.replaceState({}, "", window.location.pathname);

    if (!code || !state) {
      useAuthStore.setState({
        phase: "error",
        message: "Missing authorization code or state parameter.",
      });
      return;
    }

    completeLogin(code, state);
  }, [completeLogin]);

  useEffect(() => {
    if (phase === "ready") {
      navigate({ to: "/cabinet" });
    } else if (phase === "awaiting_identity") {
      navigate({ to: "/cabinet/devices" });
    }
  }, [phase, navigate]);

  return (
    <div className="flex min-h-screen items-center justify-center bg-base-300 font-sans">
      <div className="flex w-full max-w-md flex-col items-center gap-8 px-6 py-12">
        <OpakeLogo size="lg" />

        {(phase === "authenticating" || phase === "initializing" || phase === "unauthenticated") && (
          <div className="flex flex-col items-center gap-4">
            <span className="loading loading-spinner loading-lg text-primary" />
            <p className="text-sm text-base-content/60">Completing login…</p>
          </div>
        )}

        {phase === "error" && (
          <div className="flex flex-col items-center gap-6 text-center">
            <div className="flex h-16 w-16 items-center justify-center rounded-full bg-error/20">
              <svg
                className="h-8 w-8 text-error"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={2}
                aria-hidden="true"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M6 18L18 6M6 6l12 12"
                />
              </svg>
            </div>

            <div className="flex flex-col gap-2">
              <h1 className="text-2xl font-semibold text-base-content">
                Login failed
              </h1>
              <p className="text-base-content/60">{errorMessage}</p>
            </div>

            <a href="/login" className="btn btn-neutral btn-sm mt-2">
              Try again
            </a>
          </div>
        )}
      </div>
    </div>
  );
}

export const Route = createFileRoute("/oauth/callback")({
  beforeLoad: () => {
    const state = useAuthStore.getState();
    if (state.phase === "ready") throw redirect({ to: "/cabinet" });
  },
  component: OAuthCallbackPage,
});
