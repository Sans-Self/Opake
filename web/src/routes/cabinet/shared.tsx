import { Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import {
  UsersIcon,
  ListBulletsIcon,
  SquaresFourIcon,
  DotsThreeVerticalIcon,
  ProhibitIcon,
  DownloadSimpleIcon,
  FolderOpenIcon,
} from "@phosphor-icons/react";
import { PanelShell } from "@/components/cabinet/PanelShell";
import { PanelSkeleton } from "@/components/cabinet/PanelSkeleton";
import { PreviewPaneHeader } from "@/components/cabinet/PreviewPaneHeader";
import { FilePreview, evictPreviewCache } from "@/components/cabinet/FilePreview";
import { Breadcrumbs, BreadcrumbActive } from "@/components/cabinet/Breadcrumbs";
import { SegmentedToggle } from "@/components/SegmentedToggle";
import { DropdownMenu } from "@/components/DropdownMenu";
import { FileListRow } from "@/components/cabinet/FileListRow";
import { FileGridCard } from "@/components/cabinet/FileGridCard";
import { RevokeShareDialog } from "@/components/cabinet/RevokeShareDialog";
import type { ConfirmDialogHandle } from "@/components/ConfirmDialog";
import { isPreviewable } from "@/components/cabinet/types";
import { useAuthStore } from "@/stores/auth";
import { useDocumentsStore } from "@/stores/documents";
import { decryptDocumentRecord } from "@/stores/documents/decrypt";
import { IndexedDbStorage } from "@/lib/indexeddbStorage";
import { truncateDid, formatRelativeDate, mimeTypeToFileType, formatFileSize } from "@/lib/format";
import { handleFromDid, pdsUrlFromDid } from "@/lib/did";
import { loading as trackLoading } from "@/stores/app";
import {
  listOutgoingGrants,
  listIncomingGrants,
  revokeGrant,
  resolveIncomingGrant,
  downloadIncomingGrant,
  decryptIncomingDocument,
  type GrantEntry,
  type InboxGrantItem,
  type ResolvedIncomingGrant,
} from "@/lib/sharing";
import { decryptOwnDocument } from "@/lib/preview";
import { base64ToUint8Array } from "@/lib/encoding";
import type { OAuthSession } from "@/lib/storageTypes";
import type { FileItem } from "@/components/cabinet/types";
import { toastSuccess, toastError } from "@/stores/toast";

const storage = new IndexedDbStorage();

// ---------------------------------------------------------------------------
// Handle resolution cache
// ---------------------------------------------------------------------------

type HandleCache = Readonly<Record<string, string>>;

function useHandleResolver(dids: readonly string[]): HandleCache {
  const [cache, setCache] = useState<HandleCache>({});
  const resolvedRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    const unresolved = dids.filter((did) => !resolvedRef.current.has(did));
    if (unresolved.length === 0) return;

    unresolved.forEach((did) => {
      resolvedRef.current.add(did);
      void handleFromDid(did).then((handle) => {
        if (handle) {
          setCache((prev) => ({ ...prev, [did]: handle }));
        }
      });
    });
  }, [dids]);

  return cache;
}

// ---------------------------------------------------------------------------
// Grant → FileItem conversion
// ---------------------------------------------------------------------------

function outgoingGrantToFileItem(
  grant: GrantEntry,
  storeItem: FileItem | undefined,
  recipientDisplay: string,
): FileItem {
  const resolved = storeItem?.decrypted === true ? storeItem : undefined;
  return {
    id: grant.uri,
    uri: grant.uri,
    name: resolved?.name ?? "Encrypted file",
    kind: "file",
    fileType: resolved?.fileType,
    mimeType: resolved?.mimeType,
    encrypted: true,
    status: "shared",
    size: resolved?.size,
    modified: formatRelativeDate(grant.record.createdAt),
    decrypted: true,
    tags: [],
    subtitle: `shared with ${recipientDisplay}`,
  };
}

function incomingGrantToFileItem(
  grant: InboxGrantItem,
  ownerDisplay: string,
  resolved?: ResolvedIncomingGrant,
): FileItem {
  return {
    id: grant.uri,
    uri: grant.uri,
    name: resolved?.metadata.name ?? "Shared file",
    kind: "file",
    fileType: resolved?.metadata.mimeType
      ? mimeTypeToFileType(resolved.metadata.mimeType)
      : undefined,
    mimeType: resolved?.metadata.mimeType ?? undefined,
    size: resolved?.metadata.size != null ? formatFileSize(resolved.metadata.size) : undefined,
    encrypted: true,
    status: "shared",
    modified: formatRelativeDate(grant.createdAt),
    decrypted: resolved !== undefined,
    tags: [],
    subtitle: `from ${ownerDisplay}`,
  };
}

// ---------------------------------------------------------------------------
// Preview source discriminator
// ---------------------------------------------------------------------------

type PreviewTarget =
  | { readonly source: "outgoing"; readonly documentUri: string; readonly item: FileItem }
  | {
      readonly source: "incoming";
      readonly grant: ResolvedIncomingGrant;
      readonly item: FileItem;
    };

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

// eslint-disable-next-line sonarjs/cognitive-complexity -- layout component with split-panel preview; splitting further would obscure the data flow
function SharedPage() {
  const session = useAuthStore((s) => s.session);
  const storeItems = useDocumentsStore((s) => s.items);
  const documentRecords = useDocumentsStore((s) => s.documentRecords);
  const fetchAll = useDocumentsStore((s) => s.fetchAll);
  const cabinetPathFor = useDocumentsStore((s) => s.cabinetPathFor);
  const viewMode = useDocumentsStore((s) => s.viewMode);
  const setViewMode = useDocumentsStore((s) => s.setViewMode);
  const downloadFile = useDocumentsStore((s) => s.downloadFile);
  const navigate = useNavigate();

  const revokeDialogRef = useRef<ConfirmDialogHandle>(null);

  const [outgoing, setOutgoing] = useState<GrantEntry[]>([]);
  const [incoming, setIncoming] = useState<InboxGrantItem[]>([]);
  const [resolvedIncoming, setResolvedIncoming] = useState<Record<string, ResolvedIncomingGrant>>(
    {},
  );
  const [loading, setLoading] = useState(true);
  const [preview, setPreview] = useState<PreviewTarget | null>(null);

  // Handle resolution
  const recipientDids = useMemo(() => outgoing.map((g) => g.record.recipient), [outgoing]);
  const ownerDids = useMemo(() => incoming.map((g) => g.ownerDid), [incoming]);
  const allDids = useMemo(() => [...recipientDids, ...ownerDids], [recipientDids, ownerDids]);
  const handleCache = useHandleResolver(allDids);

  // Grant URI → GrantEntry lookup
  const grantMap = useMemo(() => new Map(outgoing.map((g) => [g.uri, g])), [outgoing]);

  // Build FileItem arrays
  const outgoingItems: readonly FileItem[] = useMemo(
    () =>
      outgoing.map((grant) => {
        const recipientDisplay =
          handleCache[grant.record.recipient] ?? truncateDid(grant.record.recipient);
        return outgoingGrantToFileItem(grant, storeItems[grant.record.document], recipientDisplay);
      }),
    [outgoing, handleCache, storeItems],
  );

  const incomingItems: readonly FileItem[] = useMemo(
    () =>
      incoming.map((grant) => {
        const ownerDisplay = handleCache[grant.ownerDid] ?? truncateDid(grant.ownerDid);
        return incomingGrantToFileItem(grant, ownerDisplay, resolvedIncoming[grant.uri]);
      }),
    [incoming, handleCache, resolvedIncoming],
  );

  const fetchGrants = useCallback(async () => {
    if (session.status !== "active") {
      setLoading(false);
      return;
    }

    const done = trackLoading("sharing-fetch");
    setLoading(true);
    try {
      const oauthSession = (await storage.loadSession(session.did)) as OAuthSession;

      const identity = await storage.loadIdentity(session.did);
      const privateKey = base64ToUint8Array(identity.private_key);
      const signingKey = identity.signing_key ? base64ToUint8Array(identity.signing_key) : null;

      const [out, inc] = await Promise.all([
        listOutgoingGrants(session.pdsUrl, session.did, oauthSession),
        signingKey
          ? listIncomingGrants(session.did, signingKey).catch((err: unknown) => {
              console.warn("[shared] inbox fetch failed, showing outgoing only:", err);
              return [] as InboxGrantItem[];
            })
          : Promise.resolve([]),
      ]);

      setOutgoing(out);
      setIncoming(inc);

      // Pre-resolve unique owner PDS URLs, then resolve each grant
      const uniqueOwnerDids = [...new Set(inc.map((g) => g.ownerDid))];
      const pdsResults = await Promise.all(
        uniqueOwnerDids.map((did) =>
          pdsUrlFromDid(did)
            .then((url) => [did, url] as const)
            .catch(() => null),
        ),
      );
      const pdsUrlCache = new Map(pdsResults.filter((r): r is NonNullable<typeof r> => r !== null));

      inc.forEach((grant) => {
        const ownerPds = pdsUrlCache.get(grant.ownerDid);
        if (!ownerPds) return;
        void resolveIncomingGrant(grant, privateKey, ownerPds)
          .then((resolved) => {
            setResolvedIncoming((prev) => ({ ...prev, [grant.uri]: resolved }));
          })
          .catch((err: unknown) => {
            console.warn("[shared] failed to resolve incoming grant:", grant.uri, err);
          });
      });
    } catch (error) {
      console.error("[shared] failed to load grants:", error);
    } finally {
      setLoading(false);
      done();
    }
  }, [session]);

  useEffect(() => {
    void fetchGrants();
  }, [fetchGrants]);

  // Ensure documents store is loaded so outgoing grant names resolve
  useEffect(() => {
    void fetchAll();
  }, [fetchAll]);

  // Decrypt metadata for outgoing grant documents that the store hasn't decrypted yet
  useEffect(() => {
    if (outgoing.length === 0 || session.status !== "active") return;

    const undecrypted = outgoing.filter((g) => {
      const item = storeItems[g.record.document];
      const record = documentRecords[g.record.document];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard on dynamic key
      return record && !item?.decrypted;
    });
    if (undecrypted.length === 0) return;

    const setItems: (fn: (draft: { items: Record<string, FileItem> }) => void) => void = (fn) => {
      useDocumentsStore.setState((state) => {
        const draft = { items: { ...state.items } };
        fn(draft);
        return { items: draft.items };
      });
    };

    void (async () => {
      const identity = await storage.loadIdentity(session.did);
      const privateKey = base64ToUint8Array(identity.private_key);
      await Promise.all(
        undecrypted.map((g) =>
          decryptDocumentRecord(
            documentRecords[g.record.document],
            session.did,
            privateKey,
            setItems,
          ).catch((err: unknown) =>
            console.warn("[shared] failed to decrypt grant document:", g.uri, err),
          ),
        ),
      );
    })();
  }, [outgoing, storeItems, documentRecords, session]);

  // Evict preview cache on close
  const previewCacheKey = preview?.source === "outgoing" ? preview.documentUri : preview?.grant.uri;
  useEffect(() => {
    if (!previewCacheKey) return undefined;
    const key = previewCacheKey;
    return () => evictPreviewCache(key);
  }, [previewCacheKey]);

  const handleRevoke = useCallback(
    async (grantUri: string) => {
      if (session.status !== "active") return;

      const done = trackLoading(`revoke:${grantUri}`);
      try {
        const oauthSession = (await storage.loadSession(session.did)) as OAuthSession;
        await revokeGrant(session.pdsUrl, session.did, grantUri, oauthSession);
        setOutgoing((prev) => prev.filter((g) => g.uri !== grantUri));
        if (preview?.item.uri === grantUri) setPreview(null);
        toastSuccess("Sharing stopped");
      } catch (error) {
        const message = error instanceof Error ? error.message : "Failed to revoke";
        toastError(message);
      } finally {
        done();
      }
    },
    [session, preview],
  );

  const handleDownloadIncoming = useCallback(
    async (item: FileItem) => {
      const resolved = resolvedIncoming[item.uri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard on dynamic key
      if (!resolved) return;

      const done = trackLoading(`download:${item.uri}`);
      try {
        await downloadIncomingGrant(resolved);
        toastSuccess("Download started");
      } catch (error) {
        const message = error instanceof Error ? error.message : "Download failed";
        toastError(message);
      } finally {
        done();
      }
    },
    [resolvedIncoming],
  );

  // Click handlers
  const handleOutgoingClick = useCallback(
    (item: FileItem) => {
      const grant = grantMap.get(item.uri);
      if (!grant) return;
      if (isPreviewable(item)) {
        setPreview({ source: "outgoing", documentUri: grant.record.document, item });
      }
    },
    [grantMap],
  );

  const handleIncomingClick = useCallback(
    (item: FileItem) => {
      const resolved = resolvedIncoming[item.uri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard on dynamic key
      if (!resolved) return;
      if (isPreviewable(item)) {
        setPreview({ source: "incoming", grant: resolved, item });
      }
    },
    [resolvedIncoming],
  );

  // Action menu renderers
  const renderIncomingActions = useCallback(
    (item: FileItem) => () => {
      const resolved = resolvedIncoming[item.uri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard on dynamic key
      if (!resolved) return null;
      return (
        // eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions -- stopPropagation wrapper
        <div onClick={(e) => e.stopPropagation()}>
          <DropdownMenu
            triggerClassName="btn btn-ghost btn-xs btn-square rounded-md"
            trigger={
              <DotsThreeVerticalIcon size={16} weight="bold" className="text-base-content" />
            }
            align="right"
            items={[
              {
                icon: DownloadSimpleIcon,
                label: "Download",
                onClick: () => void handleDownloadIncoming(item),
              },
            ]}
          />
        </div>
      );
    },
    [resolvedIncoming, handleDownloadIncoming],
  );

  const renderOutgoingActions = useCallback(
    (item: FileItem) => () => {
      const grant = grantMap.get(item.uri);
      const cabinetPath = grant ? cabinetPathFor(grant.record.document) : null;
      return (
        // eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions -- stopPropagation wrapper
        <div onClick={(e) => e.stopPropagation()}>
          <DropdownMenu
            triggerClassName="btn btn-ghost btn-xs btn-square rounded-md"
            trigger={
              <DotsThreeVerticalIcon size={16} weight="bold" className="text-base-content" />
            }
            align="right"
            items={[
              ...(cabinetPath
                ? [
                    {
                      icon: FolderOpenIcon,
                      label: "Show in cabinet",
                      onClick: () =>
                        void navigate({
                          to: "/cabinet/files/$",
                          params: { _splat: cabinetPath },
                        }),
                    },
                  ]
                : []),
              {
                icon: DownloadSimpleIcon,
                label: "Download",
                onClick: () => {
                  if (grant) void downloadFile(grant.record.document);
                },
              },
              {
                icon: ProhibitIcon,
                label: "Stop sharing",
                onClick: () => {
                  if (grant) revokeDialogRef.current?.show(grant.uri, item.name);
                },
              },
            ]}
          />
        </div>
      );
    },
    [grantMap, cabinetPathFor, downloadFile, navigate],
  );

  const FileComponent = viewMode === "list" ? FileListRow : FileGridCard;

  const breadcrumbs = (
    <Breadcrumbs>
      <BreadcrumbActive>Sharing</BreadcrumbActive>
    </Breadcrumbs>
  );

  const toolbar = (
    <SegmentedToggle
      options={[
        { value: "list" as const, icon: ListBulletsIcon },
        { value: "grid" as const, icon: SquaresFourIcon },
      ]}
      value={viewMode}
      onChange={setViewMode}
    />
  );

  const totalCount = outgoing.length + incoming.length;
  const isEmpty = totalCount === 0;

  // Preview side panel
  const handlePreviewDownload = () => {
    if (!preview) return;
    if (preview.source === "outgoing") {
      void downloadFile(preview.documentUri);
    } else {
      void handleDownloadIncoming(preview.item);
    }
  };

  const previewDecrypt =
    preview?.source === "outgoing"
      ? decryptOwnDocument(preview.documentUri)
      : preview?.source === "incoming"
        ? decryptIncomingDocument(preview.grant)
        : undefined;

  const previewPanel =
    preview && previewCacheKey && previewDecrypt ? (
      <>
        <PreviewPaneHeader
          documentName={preview.item.decrypted ? preview.item.name : null}
          onDownload={handlePreviewDownload}
          onClose={() => setPreview(null)}
        />
        <div className="min-h-0 flex-1 overflow-hidden">
          <Suspense fallback={<PanelSkeleton />}>
            <FilePreview
              cacheKey={previewCacheKey}
              decrypt={previewDecrypt}
              onDownload={handlePreviewDownload}
            />
          </Suspense>
        </div>
      </>
    ) : undefined;

  return (
    <PanelShell
      depth={1}
      breadcrumbs={breadcrumbs}
      toolbar={toolbar}
      footer={`${totalCount} shared ${totalCount === 1 ? "item" : "items"} · Encrypted`}
      sidePanel={previewPanel}
    >
      <div
        role="alert"
        className="alert border-success/30 bg-bg-sage mx-4 mt-4 gap-2.5 rounded-xl p-3"
      >
        <UsersIcon size={13} className="text-success mt-0.5 shrink-0" />
        <div>
          <div className="text-success mb-0.5 text-xs font-medium">
            End-to-end encrypted sharing
          </div>
          <div className="text-caption text-success/80 leading-relaxed">
            Only you and the people you share with can see these files.
          </div>
        </div>
      </div>

      <div className="p-3">
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
            {outgoingItems.length > 0 && (
              <section className="mb-4">
                <h3 className="text-label text-text-faint mb-2 ml-1 tracking-widest uppercase">
                  Shared by you
                </h3>
                {viewMode === "list" ? (
                  <div className="flex flex-col gap-px">
                    {outgoingItems.map((item) => (
                      <FileComponent
                        key={item.id}
                        item={item}
                        isActive={preview?.item.uri === item.uri}
                        onClick={() => handleOutgoingClick(item)}
                        hideStatus
                        renderActions={renderOutgoingActions(item)}
                      />
                    ))}
                  </div>
                ) : (
                  <div className="grid grid-cols-2 gap-3">
                    {outgoingItems.map((item) => (
                      <FileComponent
                        key={item.id}
                        item={item}
                        isActive={preview?.item.uri === item.uri}
                        onClick={() => handleOutgoingClick(item)}
                        hideStatus
                        renderActions={renderOutgoingActions(item)}
                      />
                    ))}
                  </div>
                )}
              </section>
            )}

            {incomingItems.length > 0 && (
              <section className="mb-4">
                <h3 className="text-label text-text-faint mb-2 ml-1 tracking-widest uppercase">
                  Shared with you
                </h3>
                {viewMode === "list" ? (
                  <div className="flex flex-col gap-px">
                    {incomingItems.map((item) => (
                      <FileComponent
                        key={item.id}
                        item={item}
                        isActive={preview?.item.uri === item.uri}
                        onClick={() => handleIncomingClick(item)}
                        hideStatus
                        renderActions={renderIncomingActions(item)}
                      />
                    ))}
                  </div>
                ) : (
                  <div className="grid grid-cols-2 gap-3">
                    {incomingItems.map((item) => (
                      <FileComponent
                        key={item.id}
                        item={item}
                        isActive={preview?.item.uri === item.uri}
                        onClick={() => handleIncomingClick(item)}
                        hideStatus
                        renderActions={renderIncomingActions(item)}
                      />
                    ))}
                  </div>
                )}
              </section>
            )}
          </>
        )}
      </div>

      <RevokeShareDialog
        ref={revokeDialogRef}
        onConfirm={(grantUri) => void handleRevoke(grantUri)}
      />
    </PanelShell>
  );
}

export const Route = createFileRoute("/cabinet/shared")({
  component: SharedPage,
});
