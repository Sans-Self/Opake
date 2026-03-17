import { useCallback, useEffect, useState } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { UserIcon, FloppyDiskIcon, GearIcon } from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { useAuthStore } from "@/stores/auth";
import { IndexedDbStorage } from "@/lib/indexeddbStorage";
import { authenticatedGetRecord, authenticatedPutRecord } from "@/lib/api";
import { getOpakeWorker } from "@/lib/worker";
import { truncateDid } from "@/lib/format";
import { toastSuccess, toastError } from "@/stores/toast";
import type { AccountConfigRecord } from "@/lib/pdsTypes";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const storage = new IndexedDbStorage();

// ---------------------------------------------------------------------------
// Account Config Section (PDS-synced preferences)
// ---------------------------------------------------------------------------

function AccountConfigSection() {
  const session = useAuthStore((s) => s.session);
  const [config, setConfig] = useState<AccountConfigRecord | null>(null);
  const [appviewUrl, setAppviewUrl] = useState("");
  const [savedAppviewUrl, setSavedAppviewUrl] = useState("");
  const [saving, setSaving] = useState(false);

  const did = session.status === "active" ? session.did : null;
  const pdsUrl = session.status === "active" ? session.pdsUrl : null;

  useEffect(() => {
    if (!did || !pdsUrl) return;

    const cancelled = { current: false };
    const worker = getOpakeWorker();

    void (async () => {
      const [collection, rkey, sess] = await Promise.all([
        worker.accountConfigCollection(),
        worker.accountConfigRkey(),
        storage.loadSession(did),
      ]);
      try {
        const record = await authenticatedGetRecord<AccountConfigRecord>(
          { pdsUrl, did, collection, rkey },
          sess,
        );
        if (cancelled.current) return;
        setConfig(record.value);
        const url = record.value.appviewUrl ?? "";
        setAppviewUrl(url);
        setSavedAppviewUrl(url);
      } catch (err: unknown) {
        if (cancelled.current) return;
        const is404 = err instanceof Error && err.message.includes("404");
        if (is404) {
          const defaults = await worker.newAccountConfig(new Date().toISOString());
          setConfig(defaults);
          return;
        }
        console.error("[settings] failed to load account config:", err);
      }
    })();

    return () => {
      cancelled.current = true;
    };
  }, [did, pdsUrl]);

  const saveConfig = useCallback(
    async (updates: Partial<AccountConfigRecord>) => {
      if (!did || !pdsUrl || !config) return;

      setSaving(true);
      try {
        const worker = getOpakeWorker();
        const [collection, rkey, sess] = await Promise.all([
          worker.accountConfigCollection(),
          worker.accountConfigRkey(),
          storage.loadSession(did),
        ]);
        const updated: AccountConfigRecord = {
          ...config,
          ...updates,
          modifiedAt: new Date().toISOString(),
        };
        await authenticatedPutRecord({ pdsUrl, did, collection, rkey, record: updated }, sess);
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
    [did, pdsUrl, config],
  );

  const handleToggleTelemetry = useCallback(async () => {
    if (!config) return;
    const result = await saveConfig({ telemetryEnabled: !config.telemetryEnabled });
    if (result) toastSuccess(`Telemetry ${result.telemetryEnabled ? "enabled" : "disabled"}`);
  }, [config, saveConfig]);

  const handleSaveAppviewUrl = useCallback(async () => {
    const url = appviewUrl.trim() || undefined;
    const result = await saveConfig({ appviewUrl: url });
    if (result) {
      setSavedAppviewUrl(appviewUrl);
      toastSuccess("AppView URL saved");
    }
  }, [appviewUrl, saveConfig]);

  if (!config) return null;

  const appviewDirty = appviewUrl !== savedAppviewUrl;

  return (
    <div className="card card-bordered border-base-300/50 bg-base-100 p-4">
      <div className="mb-3 flex items-center gap-2.5">
        <div className="bg-bg-stone flex size-8 shrink-0 items-center justify-center rounded-lg">
          <GearIcon size={14} className="text-text-muted" />
        </div>
        <div>
          <div className="text-ui text-base-content font-medium">Preferences</div>
          <div className="text-caption text-text-muted">Synced to your PDS across devices.</div>
        </div>
      </div>

      <div className="flex flex-col gap-3">
        <label className="flex cursor-pointer items-center justify-between gap-3">
          <div>
            <div className="text-ui text-base-content">Telemetry</div>
            <div className="text-caption text-text-muted">Usage analytics — not yet active</div>
          </div>
          <input
            type="checkbox"
            className="toggle toggle-sm toggle-primary"
            checked={config.telemetryEnabled}
            disabled={saving}
            onChange={() => void handleToggleTelemetry()}
            aria-label="Enable telemetry"
          />
        </label>

        <div>
          <div className="text-ui text-base-content mb-1">AppView URL</div>
          <div className="flex items-center gap-2">
            <input
              type="url"
              placeholder="https://appview.opake.app"
              value={appviewUrl}
              onChange={(e) => setAppviewUrl(e.target.value)}
              className="input input-bordered input-sm border-base-300/50 bg-base-200/50 text-ui flex-1"
              aria-label="AppView URL"
            />
            <button
              onClick={() => void handleSaveAppviewUrl()}
              disabled={!appviewDirty || saving}
              className="btn btn-primary btn-sm gap-1.5"
            >
              <FloppyDiskIcon size={13} />
              {saving ? "Saving…" : "Save"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Account Section
// ---------------------------------------------------------------------------

function AccountSection() {
  const session = useAuthStore((s) => s.session);

  if (session.status !== "active") return null;

  return (
    <div className="card card-bordered border-base-300/50 bg-base-100 p-4">
      <div className="mb-3 flex items-center gap-2.5">
        <div className="bg-bg-stone flex size-8 shrink-0 items-center justify-center rounded-lg">
          <UserIcon size={14} className="text-text-muted" />
        </div>
        <div>
          <div className="text-ui text-base-content font-medium">Account</div>
          <div className="text-caption text-text-muted">Your AT Protocol identity</div>
        </div>
      </div>

      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-xs">
        <dt className="text-text-faint font-medium">Handle</dt>
        <dd className="text-base-content font-mono">{session.handle}</dd>

        <dt className="text-text-faint font-medium">DID</dt>
        <dd className="text-base-content font-mono" title={session.did}>
          {truncateDid(session.did)}
        </dd>

        <dt className="text-text-faint font-medium">PDS</dt>
        <dd className="text-base-content font-mono">{session.pdsUrl}</dd>
      </dl>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Settings Page
// ---------------------------------------------------------------------------

function SettingsPage() {
  const breadcrumbs = (
    <div className="breadcrumbs text-ui min-w-0 flex-1 overflow-hidden">
      <ul>
        <li>
          <span className="text-base-content font-medium">Settings</span>
        </li>
      </ul>
    </div>
  );

  return (
    <PanelShell depth={1} breadcrumbs={breadcrumbs} footer="Account settings">
      <div className="p-5">
        <div className="mb-5">
          <div className="text-ui text-base-content mb-1 font-medium">Settings</div>
          <div className="text-text-muted text-xs">Manage your account, keys, and preferences.</div>
        </div>
        <div className="divider mt-0 mb-4" />

        <div className="flex flex-col gap-3">
          <AccountSection />
          <AccountConfigSection />
        </div>
      </div>
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/settings")({
  component: SettingsPage,
});
