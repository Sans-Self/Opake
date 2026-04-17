import { useCallback, useEffect, useState } from "react";
import { createLazyFileRoute } from "@tanstack/react-router";
import { FloppyDiskIcon } from "@phosphor-icons/react";
import type { AccountConfigPatch } from "@opake/sdk";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { getOpake, useAuthStore } from "@/stores/auth";
import { truncateDid } from "@/lib/format";
import { toastSuccess, toastError } from "@/stores/toast";

function SettingsPage() {
  const session = useAuthStore((s) => s.session);
  const did = session.status === "active" ? session.did : null;
  const handle = session.status === "active" ? session.handle : null;
  const pdsUrl = session.status === "active" ? session.pdsUrl : null;

  const [config, setConfig] = useState<import("@opake/sdk").AccountConfig | null>(null);
  const [appviewUrl, setAppviewUrl] = useState("");
  const [savedAppviewUrl, setSavedAppviewUrl] = useState("");
  const [saving, setSaving] = useState(false);

  // Load the persisted config once per session. The cancelled flag
  // silences setState-after-unmount under React StrictMode double-fire.
  useEffect(() => {
    if (!did) return;

    const cancelled = { current: false };

    void (async () => {
      try {
        const result = await getOpake().getAccountConfig();
        if (cancelled.current) return;
        const url = result?.appviewUrl ?? "";
        setConfig(result);
        setAppviewUrl(url);
        setSavedAppviewUrl(url);
      } catch (err) {
        if (cancelled.current) return;
        console.error("[settings] failed to load account config:", err);
      }
    })();

    return () => {
      cancelled.current = true;
    };
  }, [did]);

  const handleAppviewSave = useCallback(() => {
    if (!did) return;
    const trimmed = appviewUrl.trim();

    // Validate URL before saving — a malicious URL would receive Ed25519
    // auth signatures that could be replayed within the 60s window.
    if (trimmed.length > 0) {
      try {
        const parsed = new URL(trimmed);
        if (parsed.protocol !== "https:") {
          toastError("AppView URL must use HTTPS");
          return;
        }
      } catch {
        toastError("Invalid URL");
        return;
      }
    }

    setSaving(true);
    void (async () => {
      try {
        const patch: AccountConfigPatch = {
          // Empty field → explicit null (clear the stored override).
          // Non-empty → set the new URL. Never undefined, which would
          // leave the current value untouched instead of clearing it.
          appviewUrl: trimmed.length > 0 ? trimmed : null,
        };
        const updated = await getOpake().updateAccountConfig(patch);
        setConfig(updated);
        setSavedAppviewUrl(updated.appviewUrl ?? "");
        toastSuccess("AppView URL saved");
      } catch (err) {
        toastError(err instanceof Error ? err.message : "Failed to save");
      } finally {
        setSaving(false);
      }
    })();
  }, [appviewUrl, did]);

  const appviewDirty = appviewUrl !== savedAppviewUrl;

  const breadcrumbs = <span>Settings</span>;

  if (!did || !handle || !pdsUrl) {
    return (
      <PanelShell depth={0} breadcrumbs={breadcrumbs} footer="">
        <div className="text-base-content/50 flex h-full items-center justify-center">
          Log in to view settings
        </div>
      </PanelShell>
    );
  }

  return (
    <PanelShell depth={0} breadcrumbs={breadcrumbs} footer="">
      <div className="mx-auto max-w-lg space-y-8 px-6 py-6">
        <h1 className="text-base-content text-lg font-semibold">Settings</h1>

        {/* Account info */}
        <section>
          <h2 className="text-base-content mb-3 text-sm font-semibold">Account</h2>
          <div className="space-y-3">
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
          </div>
        </section>

        {/* AppView URL */}
        <section>
          <h2 className="text-base-content mb-3 text-sm font-semibold">AppView URL</h2>
          <div className="space-y-3">
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
                disabled={saving}
              />
              <button
                type="button"
                className="btn btn-sm btn-primary gap-1.5"
                disabled={!appviewDirty || saving}
                onClick={handleAppviewSave}
              >
                <FloppyDiskIcon size={16} /> Save
              </button>
            </div>
            {config && (
              <p className="text-base-content/40 text-xs">
                Last saved: {new Date(config.modifiedAt).toLocaleString()}
              </p>
            )}
          </div>
        </section>
      </div>
    </PanelShell>
  );
}

export const Route = createLazyFileRoute("/cabinet/settings")({
  component: SettingsPage,
});
