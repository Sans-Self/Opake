import { createLazyFileRoute } from "@tanstack/react-router";
import { useAuthStore } from "@/stores/auth";
import { OpakeLogo } from "@/components/OpakeLogo";
import { FreshAccountView } from "@/components/devices/FreshAccountView";
import { RecoverIdentityView } from "@/components/devices/RecoverIdentityView";
import { ConflictView } from "@/components/devices/ConflictView";
import { ReadyView } from "@/components/devices/ReadyView";

function DevicesPage() {
  const identity = useAuthStore((s) => s.identity);

  switch (identity.status) {
    case "pending":
      return (
        <div className="flex flex-col items-center gap-4">
          <OpakeLogo size="xl" loading />
          <p className="text-base-content/60 text-sm">Checking encryption key…</p>
        </div>
      );
    case "none":
    case "fresh":
      return <FreshAccountView />;
    case "remote_only":
      return <RecoverIdentityView />;
    case "conflict":
      return <ConflictView />;
    case "ready":
      return <ReadyView />;
  }
}

export const Route = createLazyFileRoute("/devices/")({
  component: DevicesPage,
});
