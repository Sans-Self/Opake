import { useEffect, useRef } from "react";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { useAuthStore } from "@/stores/auth";

function OAuthCallbackPage() {
  const navigate = useNavigate();
  const session = useAuthStore((s) => s.session);
  const completeLogin = useAuthStore((s) => s.completeLogin);
  const errorMessage = session.status === "error" ? session.message : null;

  const hasStartedRef = useRef(false);

  useEffect(() => {
    if (hasStartedRef.current) return;
    hasStartedRef.current = true;

    const params = new URLSearchParams(window.location.search);
    const code = params.get("code");
    const state = params.get("state");

    window.history.replaceState({}, "", window.location.pathname);

    if (!code || !state) {
      useAuthStore.setState((draft) => {
        draft.session = {
          status: "error",
          message: "Missing authorization code or state parameter.",
        };
      });
      return;
    }

    void completeLogin(code, state);
  }, [completeLogin]);

  useEffect(() => {
    if (session.status === "active") {
      void navigate({ to: "/devices" });
    }
  }, [session.status, navigate]);

  return (
    <>
      {session.status !== "error" && (
        <div className="flex flex-col items-center gap-4">
          <span className="loading loading-spinner loading-lg text-primary" />
          <p className="text-base-content/60 text-sm">Completing login…</p>
        </div>
      )}

      {session.status === "error" && (
        <div className="flex flex-col items-center gap-6 text-center">
          <div className="bg-error/20 flex h-16 w-16 items-center justify-center rounded-full">
            <svg
              className="text-error h-8 w-8"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
              aria-hidden="true"
            >
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </div>

          <div className="flex flex-col gap-2">
            <h1 className="text-base-content text-2xl font-semibold">Login failed</h1>
            <p className="text-base-content/60">{errorMessage}</p>
          </div>

          <a href="/devices/login" className="btn btn-neutral btn-sm mt-2">
            Try again
          </a>
        </div>
      )}
    </>
  );
}

export const Route = createFileRoute("/devices/oauth-callback")({
  beforeLoad: async () => {
    const state = useAuthStore.getState();
    if (state.session.status === "initializing") {
      await state.boot();
    }
    if (useAuthStore.getState().session.status === "active") {
      throw redirect({ to: "/devices" });
    }
  },
  component: OAuthCallbackPage,
});
