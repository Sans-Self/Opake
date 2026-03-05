import { useEffect } from "react";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useAuthStore } from "@/stores/auth";

function OAuthCallbackPage() {
  const navigate = useNavigate();
  const phase = useAuthStore((s) => s.phase);
  const completeLogin = useAuthStore((s) => s.completeLogin);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const code = params.get("code");
    const state = params.get("state");
    const error = params.get("error");

    if (error) {
      const description = params.get("error_description") ?? error;
      navigate({ to: "/login", search: { error: description } });
      return;
    }

    if (!code || !state) {
      navigate({ to: "/login", search: { error: "Missing OAuth parameters" } });
      return;
    }

    completeLogin(code, state);
  }, [completeLogin, navigate]);

  useEffect(() => {
    if (phase === "ready") {
      navigate({ to: "/cabinet" });
    } else if (phase === "error") {
      const state = useAuthStore.getState();
      const message = "message" in state ? state.message : "Authentication failed";
      navigate({ to: "/login", search: { error: message } });
    }
  }, [phase, navigate]);

  return (
    <div className="flex min-h-screen items-center justify-center bg-base-300 font-sans">
      <div className="flex flex-col items-center gap-4">
        <span className="loading loading-spinner loading-lg" />
        <p className="text-sm text-text-muted">Completing authentication…</p>
      </div>
    </div>
  );
}

export const Route = createFileRoute("/oauth/callback")({
  component: OAuthCallbackPage,
});
