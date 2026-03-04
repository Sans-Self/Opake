import { useState } from "react";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { OpakeLogo } from "@/components/OpakeLogo";
import { useAuthStore } from "@/stores/auth";

function LoginPage() {
  const navigate = useNavigate();
  const login = useAuthStore((s) => s.login);
  const [handle, setHandle] = useState("alice.bsky.social");

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    await login(handle, "");
    navigate({ to: "/cabinet" });
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
          Enter your PDS handle to continue.
        </p>
        <label className="input input-bordered mb-3 flex items-center gap-2">
          <input
            type="text"
            placeholder="handle.bsky.social"
            value={handle}
            onChange={(e) => setHandle(e.target.value)}
            className="grow"
            required
          />
        </label>
        <button type="submit" className="btn btn-neutral w-full">
          Sign in
        </button>
      </form>
    </div>
  );
}

export const Route = createFileRoute("/login")({
  beforeLoad: () => {
    const { currentDid } = useAuthStore.getState();
    if (currentDid) throw redirect({ to: "/cabinet" });
  },
  component: LoginPage,
});
