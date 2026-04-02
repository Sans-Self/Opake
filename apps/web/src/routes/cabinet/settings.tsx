import { useCallback, useEffect, useState } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { UserIcon, FloppyDiskIcon, GearIcon } from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { useAuthStore } from "@/stores/auth";
import { getOpakeWorker } from "@/lib/worker";
import { truncateDid } from "@/lib/format";
import { toastSuccess, toastError } from "@/stores/toast";
import type { AccountConfigRecord } from "@/lib/pdsTypes";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function SettingsPage() {
  const session = useAuthStore((s) => s.session);
  const did = session.status === "active" ? session.did : null;
  const handle = session.status === "active" ? session.handle : null;
  const pdsUrl = session.status === "active" ? session.pdsUrl : null;

  const [config, setConfig] = useState<AccountConfigRecord | null>(null);
  const [appviewUrl, setAppviewUrl] = useState("");
  const [savedAppviewUrl, setSavedAppviewUrl] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!did) return;

    const cancelled = { current: false };
    const worker = getOpakeWorker();

    void (async () => {
      try {
        const result = (await worker.getAccountConfig()) as AccountConfigRecord | null;
        if (cancelled.current) return;
        if (result) {
          setConfig(result);
          const url = result.appviewUrl ?? "";
          setAppviewUrl(url);
          setSavedAppviewUrl(url);
        } else {
          const defaults = await worker.newAccountConfig(new Date().toISOString());
          setConfig(defaults);
        }
      } catch (err: unknown) {
        if (cancelled.current) return;
        console.error("[settings] failed to load account config:", err);
      }
    })();

    return () => {
      cancelled.current = true;
    };
  }, [did]);

  const saveConfig = useCallback(
    async (updates: Partial<AccountConfigRecord>) => {
      if (!did || !config) return;

      setSaving(true);
      try {
        const worker = getOpakeWorker();
        const updated: AccountConfigRecord = {
          ...config,
          ...updates,
          modifiedAt: new Date().toISOString(),
        };
        await worker.setAccountConfig(updated);
        setConfig(updated);
        return updated;
      } catch (error) {
        const message = error instanceof Error ? error.message : "Failed to save";
        toastError(message);
        return undefined;
      } finally {
        setSaving(false);
      }
    },
    [did, config],
  );

  const handleAppviewSave = useCallback(async () => {
    const result = await saveConfig({
      appviewUrl: appviewUrl.trim() || undefined,
    });
    if (result) {
      setSavedAppviewUrl(result.appviewUrl ?? "");
      toastSuccess("AppView URL saved");
    }
  }, [appviewUrl, saveConfig]);

  const appviewDirty = appviewUrl !== savedAppviewUrl;

  if (!did || !handle || !pdsUrl) {
    return (
      <PanelShell depth={0} breadcrumbs={<span>Settings</span>} footer="">
        <div className="text-base-content/50 flex h-full items-center justify-center">
          Log in to view settings
        </div>
      </PanelShell>
    );
  }

  return (
    <PanelShell depth={0} breadcrumbs={<span>Settings</span>} footer="">
      <div className="mx-auto max-w-2xl space-y-8 p-6">
        <h1 className="flex items-center gap-2 text-2xl font-bold">
          <GearIcon size={24} /> Settings
        </h1>

        {/* Account info */}
        <section className="card bg-base-200 space-y-2 p-4">
          <h2 className="flex items-center gap-2 font-semibold">
            <UserIcon size={18} /> Account
          </h2>
          <div className="space-y-1 text-sm">
            <div>
              <span className="text-base-content/60">Handle:</span>{" "}
              <span className="font-mono">{handle}</span>
            </div>
            <div>
              <span className="text-base-content/60">DID:</span>{" "}
              <span className="font-mono text-xs">{truncateDid(did)}</span>
            </div>
            <div>
              <span className="text-base-content/60">PDS:</span>{" "}
              <span className="font-mono text-xs">{pdsUrl}</span>
            </div>
          </div>
        </section>

        {/* AppView URL */}
        <section className="card bg-base-200 space-y-3 p-4">
          <h2 className="font-semibold">AppView URL</h2>
          <p className="text-base-content/60 text-sm">
            The AppView indexes workspace membership and incoming shares. Leave blank to use the
            default.
          </p>
          <div className="flex gap-2">
            <input
              type="url"
              className="input input-bordered input-sm flex-1"
              placeholder="https://appview.opake.app"
              value={appviewUrl}
              onChange={(e) => setAppviewUrl(e.target.value)}
            />
            <button
              type="button"
              className="btn btn-sm btn-primary"
              disabled={!appviewDirty || saving}
              onClick={() => void handleAppviewSave()}
            >
              <FloppyDiskIcon size={16} /> Save
            </button>
          </div>
        </section>
      </div>
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/settings")({
  component: SettingsPage,
});
