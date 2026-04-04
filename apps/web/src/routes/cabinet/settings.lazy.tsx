import { createLazyFileRoute } from "@tanstack/react-router";
import { useAuthStore } from "@/stores/auth";

function Settings() {
  const session = useAuthStore((s) => s.session);
  const did = session.status === "active" ? session.did : null;
  const handle = session.status === "active" ? session.handle : null;
  const pdsUrl = session.status === "active" ? session.pdsUrl : null;

  return (
    <div className="flex flex-1 flex-col gap-6 overflow-auto p-6">
      <h1 className="text-base-content text-lg font-semibold">Settings</h1>
      <div className="text-base-content/60 space-y-1 text-sm">
        {did && (
          <p>
            DID: <span className="font-mono text-xs">{did}</span>
          </p>
        )}
        {handle && (
          <p>
            Handle: <span className="font-mono text-xs">{handle}</span>
          </p>
        )}
        {pdsUrl && (
          <p>
            PDS: <span className="font-mono text-xs">{pdsUrl}</span>
          </p>
        )}
      </div>
      <p className="text-base-content/40 text-sm">Settings — not yet wired to SDK</p>
    </div>
  );
}

export const Route = createLazyFileRoute("/cabinet/settings")({
  component: Settings,
});
