import { useCallback, useEffect, useState } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { UserIcon, GlobeIcon, FloppyDiskIcon } from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { useAuthStore } from "@/stores/auth";
import { IndexedDbStorage } from "@/lib/indexeddbStorage";
import { truncateDid } from "@/lib/format";
import { toastSuccess, toastError } from "@/stores/toast";
import type { Config } from "@/lib/storageTypes";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const storage = new IndexedDbStorage();

// ---------------------------------------------------------------------------
// AppView URL Section
// ---------------------------------------------------------------------------

function AppViewUrlSection() {
  const [appviewUrl, setAppviewUrl] = useState("");
  const [savedUrl, setSavedUrl] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    storage
      .loadConfig()
      .then((config) => {
        const url = config.appviewUrl ?? "";
        setAppviewUrl(url);
        setSavedUrl(url);
      })
      .catch(() => {
        // No config yet — leave empty
      });
  }, []);

  const isDirty = appviewUrl !== savedUrl;

  const handleSave = useCallback(async () => {
    setSaving(true);
    try {
      const config: Config = await storage.loadConfig().catch(
        (): Config => ({
          defaultDid: null,
          accounts: {},
          appviewUrl: null,
        }),
      );
      const updated: Config = {
        ...config,
        appviewUrl: appviewUrl.trim() || null,
      };
      await storage.saveConfig(updated);
      setSavedUrl(appviewUrl);
      toastSuccess("AppView URL saved");
    } catch (error) {
      const message = error instanceof Error ? error.message : "Failed to save";
      toastError(message);
    } finally {
      setSaving(false);
    }
  }, [appviewUrl]);

  return (
    <div className="card card-bordered border-base-300/50 bg-base-100 p-4">
      <div className="mb-3 flex items-center gap-2.5">
        <div className="bg-bg-stone flex size-8 shrink-0 items-center justify-center rounded-lg">
          <GlobeIcon size={14} className="text-text-muted" />
        </div>
        <div>
          <div className="text-ui text-base-content font-medium">AppView URL</div>
          <div className="text-caption text-text-muted">
            The AppView endpoint used for indexing and search.
          </div>
        </div>
      </div>

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
          onClick={() => void handleSave()}
          disabled={!isDirty || saving}
          className="btn btn-primary btn-sm gap-1.5"
        >
          <FloppyDiskIcon size={13} />
          {saving ? "Saving…" : "Save"}
        </button>
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
          <AppViewUrlSection />
        </div>
      </div>
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/settings")({
  component: SettingsPage,
});
