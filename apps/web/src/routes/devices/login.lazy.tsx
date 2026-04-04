import { useState } from "react";
import { createLazyFileRoute } from "@tanstack/react-router";
import { useAuthStore } from "@/stores/auth";

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
    <form onSubmit={handleSubmit} className="card card-bordered bg-base-100 w-80 p-6">
      <h1 className="text-ui text-base-content mb-1 font-medium">Sign in to Opake</h1>
      <p className="text-caption text-text-muted mb-5">
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
      {errorMessage && (
        <p className="text-caption text-error mb-3" role="alert">
          {errorMessage}
        </p>
      )}
      <button type="submit" className="btn btn-neutral w-full" disabled={isLoading}>
        {isLoading ? <span className="loading loading-spinner loading-sm" /> : "Sign in"}
      </button>
    </form>
  );
}

export const Route = createLazyFileRoute("/devices/login")({
  component: LoginPage,
});
