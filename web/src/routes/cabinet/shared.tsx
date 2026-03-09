import { useCallback, useEffect, useState } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { UsersIcon, ShareNetworkIcon, TrashIcon, ArrowSquareOutIcon } from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { useAuthStore } from "@/stores/auth";
import { IndexedDbStorage } from "@/lib/indexeddbStorage";
import { truncateDid, formatShortDate } from "@/lib/format";
import {
  listOutgoingGrants,
  listIncomingGrants,
  revokeGrant,
  type GrantEntry,
  type InboxGrantItem,
} from "@/lib/sharing";
import type { OAuthSession } from "@/lib/storageTypes";
import { toastSuccess, toastError } from "@/stores/toast";

const storage = new IndexedDbStorage();

// ---------------------------------------------------------------------------
// Outgoing Grants Section
// ---------------------------------------------------------------------------

function OutgoingGrantsSection({
  grants,
  onRevoke,
}: {
  readonly grants: readonly GrantEntry[];
  readonly onRevoke: (uri: string) => void;
}) {
  if (grants.length === 0) return null;

  return (
    <section className="mb-6">
      <h3 className="text-label text-text-faint mb-2 ml-1 tracking-widest uppercase">
        Shared by you
      </h3>
      <div className="flex flex-col gap-1">
        {grants.map((grant) => (
          <div
            key={grant.uri}
            className="hover:bg-bg-hover flex items-center gap-3 rounded-xl px-3 py-2.5 transition-colors"
          >
            <div className="bg-primary/10 flex size-8 shrink-0 items-center justify-center rounded-lg">
              <ShareNetworkIcon size={14} className="text-primary" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="text-ui text-base-content truncate">
                → {truncateDid(grant.record.recipient)}
              </div>
              <div className="text-caption text-text-faint mt-0.5">
                {formatShortDate(grant.record.createdAt)}
              </div>
            </div>
            <button
              onClick={() => onRevoke(grant.uri)}
              className="btn btn-ghost btn-xs btn-square rounded-md"
              aria-label={`Revoke grant to ${grant.record.recipient}`}
            >
              <TrashIcon size={13} className="text-error" />
            </button>
          </div>
        ))}
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------
// Incoming Grants Section
// ---------------------------------------------------------------------------

function IncomingGrantsSection({ grants }: { readonly grants: readonly InboxGrantItem[] }) {
  if (grants.length === 0) return null;

  return (
    <section className="mb-6">
      <h3 className="text-label text-text-faint mb-2 ml-1 tracking-widest uppercase">
        Shared with you
      </h3>
      <div className="flex flex-col gap-1">
        {grants.map((grant) => (
          <div
            key={grant.uri}
            className="hover:bg-bg-hover flex items-center gap-3 rounded-xl px-3 py-2.5 transition-colors"
          >
            <div className="bg-success/10 flex size-8 shrink-0 items-center justify-center rounded-lg">
              <UsersIcon size={14} className="text-success" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="text-ui text-base-content truncate">
                From {truncateDid(grant.ownerDid)}
              </div>
              <div className="text-caption text-text-faint mt-0.5">
                {formatShortDate(grant.createdAt)}
              </div>
            </div>
            <ArrowSquareOutIcon size={13} className="text-text-faint shrink-0" />
          </div>
        ))}
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

function SharedPage() {
  const session = useAuthStore((s) => s.session);
  const [outgoing, setOutgoing] = useState<GrantEntry[]>([]);
  const [incoming, setIncoming] = useState<InboxGrantItem[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchGrants = useCallback(async () => {
    if (session.status !== "active") {
      setLoading(false);
      return;
    }

    setLoading(true);
    try {
      const oauthSession = (await storage.loadSession(session.did)) as OAuthSession;

      const [out, inc] = await Promise.all([
        listOutgoingGrants(session.pdsUrl, session.did, oauthSession),
        listIncomingGrants(session.did),
      ]);

      setOutgoing(out);
      setIncoming(inc);
    } catch (error) {
      console.error("[shared] failed to load grants:", error);
    } finally {
      setLoading(false);
    }
  }, [session]);

  useEffect(() => {
    void fetchGrants();
  }, [fetchGrants]);

  const handleRevoke = useCallback(
    async (grantUri: string) => {
      if (session.status !== "active") return;

      try {
        const oauthSession = (await storage.loadSession(session.did)) as OAuthSession;
        await revokeGrant(session.pdsUrl, session.did, grantUri, oauthSession);
        setOutgoing((prev) => prev.filter((g) => g.uri !== grantUri));
        toastSuccess("Grant revoked");
      } catch (error) {
        const message = error instanceof Error ? error.message : "Failed to revoke";
        toastError(message);
      }
    },
    [session],
  );

  const breadcrumbs = (
    <div className="breadcrumbs text-ui min-w-0 flex-1 overflow-hidden">
      <ul>
        <li>
          <span className="text-base-content font-medium">Shared with me</span>
        </li>
      </ul>
    </div>
  );

  const isEmpty = outgoing.length === 0 && incoming.length === 0;

  return (
    <PanelShell depth={1} breadcrumbs={breadcrumbs} footer="Shared items · Encrypted">
      <div
        role="alert"
        className="alert border-success/30 bg-bg-sage mx-4 mt-4 gap-2.5 rounded-xl p-3"
      >
        <UsersIcon size={13} className="text-success mt-0.5 shrink-0" />
        <div>
          <div className="text-success mb-0.5 text-xs font-medium">
            Shared via decentralised identity
          </div>
          <div className="text-caption text-success/80 leading-relaxed">
            Files shared via DID. Encrypted in transit and at rest — only invited parties can
            decrypt.
          </div>
        </div>
      </div>

      <div className="p-4">
        {loading ? (
          <div className="flex justify-center py-12">
            <span className="loading loading-spinner loading-md text-text-faint" />
          </div>
        ) : isEmpty ? (
          <div className="hero py-16">
            <div className="hero-content flex-col text-center">
              <div className="bg-bg-sage flex size-13 items-center justify-center rounded-[14px]">
                <UsersIcon size={22} className="text-success" />
              </div>
              <div className="text-ui text-text-muted">No shared files yet</div>
              <div className="text-caption text-text-faint max-w-xs">
                Share files from the file action menu to grant access to other Opake users.
              </div>
            </div>
          </div>
        ) : (
          <>
            <OutgoingGrantsSection grants={outgoing} onRevoke={(uri) => void handleRevoke(uri)} />
            <IncomingGrantsSection grants={incoming} />
          </>
        )}
      </div>
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/shared")({
  component: SharedPage,
});
