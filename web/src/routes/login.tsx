import { createFileRoute } from "@tanstack/react-router";

function LoginPage() {
  return (
    <div>
      <h1 className="text-xl font-semibold">Login</h1>
      <p className="mt-2 text-neutral-500">Authentication flow goes here.</p>
    </div>
  );
}

export const Route = createFileRoute("/login")({
  component: LoginPage,
});
