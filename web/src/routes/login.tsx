import { useState } from "react";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { OpakeLogo } from "@/components/OpakeLogo";
import { useAuthStore } from "@/stores/auth";

function LoginPage() {
  const navigate = useNavigate();
  const phase = useAuthStore((s) => s.phase);
  const startLogin = useAuthStore((s) => s.startLogin);
  const [handle, setHandle] = useState("");
  const isLoading = phase === "authenticating";
  const errorMessage = phase === "error" ? useAuthStore.getState() : null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!handle.trim()) return;
    await startLogin(handle.trim());
    // startLogin redirects to the AS — we won't reach here unless it errors
  };

  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-base-300 font-sans">
      <div className="mb-8">
        <OpakeLogo size="lg" />
      </div>
      <form
        onSubmit={handleSubmit}
        className="card card-bordered w-80 bg-base-100 p-6"
      >
        <h1 className="mb-1 text-ui font-medium text-base-content">
          Sign in to Opake
        </h1>
        <p className="mb-5 text-caption text-text-muted">
          Enter your AT Protocol handle to continue.
        </p>
        <label className="input input-bordered mb-3 flex items-center gap-2">
          <input
            type="text"
            placeholder="you.bsky.social"
            value={handle}
            onChange={(e) => setHandle(e.target.value)}
            className="grow"
            required
            disabled={isLoading}
            aria-label="AT Protocol handle"
          />
        </label>
        {phase === "error" && errorMessage && (
          <p className="mb-3 text-caption text-error" role="alert">
            {"message" in errorMessage ? errorMessage.message : "Login failed"}
          </p>
        )}
        <button
          type="submit"
          className="btn btn-neutral w-full"
          disabled={isLoading}
        >
          {isLoading ? (
            <span className="loading loading-spinner loading-sm" />
          ) : (
            "Sign in"
          )}
        </button>
      </form>
    </div>
  );
}

export const Route = createFileRoute("/login")({
  beforeLoad: () => {
    const state = useAuthStore.getState();
    if (state.phase === "ready") throw redirect({ to: "/cabinet" });
  },
  component: LoginPage,
});
