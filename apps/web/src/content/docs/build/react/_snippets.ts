// Code snippets for the @opake/react docs pages.
// MDX 3 dedents multi-line template literals inside .mdx files; a .ts
// import bypasses that. See build/sdk/_snippets.ts for context.

// -- react/overview.mdx -----------------------------------------------------

export const providerSetup = `import { Opake } from "@opake/sdk";
import { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";
import { OpakeProvider } from "@opake/react";

const storage = new IndexedDbStorage();
const opake = await Opake.init({ storage });

// Once per app, at or near the root. Everything under it gets useOpake,
// useFileManager, useDirectory, plus the subscription-backed hooks.
<OpakeProvider opake={opake}>
  <App />
</OpakeProvider>`;

export const helloHook = `import { useOpake, useDirectory } from "@opake/react";

function CabinetRoot() {
  const opake = useOpake(); // raw Opake instance, for things not wrapped yet

  // Live-subscribed snapshot of the cabinet's root directory. Pass null
  // for the keyringUri (= cabinet) and null for the directoryUri (= root).
  // Re-renders on every upload, rename, move, delete, or remote change.
  const { snapshot, isReady } = useDirectory(null, null);

  if (!isReady) return <p>Loading\u2026</p>;
  if (!snapshot?.rootUri) return <p>Empty cabinet.</p>;

  const root = snapshot.directories[snapshot.rootUri];
  return (
    <ul>
      {root?.entries.map((entry) => (
        <li key={entry.uri}>{entry.uri.split("/").pop()}</li>
      ))}
    </ul>
  );
}`;

export const providerWithQueryClient = `import { QueryClient } from "@tanstack/react-query";
import { OpakeProvider } from "@opake/react";

// If your app already has a QueryClient (most TanStack-using apps do),
// pass it in. OpakeProvider registers its own queries inside your
// shared client instead of maintaining a parallel one.
const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 60_000 } },
});

<OpakeProvider opake={opake} queryClient={queryClient}>
  <App />
</OpakeProvider>`;

export const providerDisableSse = `// Rare: you're gating the SSE consumer on something app-specific
// (feature flag, explicit user opt-in, offline mode) and want to start
// it yourself.
<OpakeProvider opake={opake} disableSseAutoStart>
  <App />
</OpakeProvider>

// Somewhere else, when you're ready:
import { useStartSseConsumer } from "@opake/react";

function OfflineAwareGate() {
  const online = useNavigatorOnline();
  useStartSseConsumer(online ? undefined : null);
  return <Outlet />;
}`;

// -- react/queries.mdx ------------------------------------------------------

export const useDirectoryExample = `import { useDirectory } from "@opake/react";

function Directory({ keyringUri, uri }: {
  keyringUri: string | null; // null for cabinet, keyring URI for a workspace
  uri: string | null;        // null to watch the context's root
}) {
  const { snapshot, isReady, error, retry } = useDirectory(keyringUri, uri);

  if (error) return <RetryBanner message={error.message} onRetry={retry} />;
  if (!isReady) return <p>Loading\u2026</p>;
  if (!snapshot) return <p>This directory no longer exists.</p>;

  return <DirectoryTreeView snapshot={snapshot} focusedUri={uri ?? snapshot.rootUri} />;
}`;

export const useDirectoryMetadataExample = `import { useDirectoryMetadata } from "@opake/react";

function DirectoryFilenames({ keyringUri, directoryUri }: {
  keyringUri: string | null;
  directoryUri: string;
}) {
  // Thin read: just the decrypted metadata for documents in one
  // directory (filename, MIME type, size, tags, descriptions).
  const { data: metadata, isLoading } = useDirectoryMetadata(keyringUri, directoryUri);
  if (isLoading) return <p>Loading\u2026</p>;

  return (
    <ul>
      {Object.entries(metadata ?? {}).map(([docUri, meta]) => (
        <li key={docUri}>
          <a href={\`/doc/\${encodeURIComponent(docUri)}\`}>{meta.name}</a>
          <small> ({meta.mimeType}, {meta.size} bytes)</small>
        </li>
      ))}
    </ul>
  );
}`;

export const useWorkspacesExample = `import { useWorkspaces } from "@opake/react";

function WorkspaceList() {
  const { data: workspaces, isLoading } = useWorkspaces();
  if (isLoading) return <p>Loading workspaces\u2026</p>;

  return (
    <ul>
      {workspaces.map((ws) => (
        <li key={ws.uri}>
          {ws.name || "Unnamed workspace"}{" "}
          <small>({ws.memberCount} member{ws.memberCount === 1 ? "" : "s"})</small>
        </li>
      ))}
    </ul>
  );
}`;

export const useInboxExample = `import { useInbox } from "@opake/react";

function Inbox() {
  const { data: inbox, isLoading } = useInbox();
  if (isLoading) return <p>Loading inbox\u2026</p>;

  return (
    <ul>
      {inbox.map((grant) => (
        <li key={grant.uri}>
          <code>{grant.documentUri}</code>{" "}
          <small>from {grant.authorDid}</small>
        </li>
      ))}
    </ul>
  );
}`;

export const useSharesExample = `import { useShares } from "@opake/react";

function ShareList({ documentUri }: { documentUri: string }) {
  const { data: grants, isLoading } = useShares(documentUri);
  if (isLoading) return <p>Loading\u2026</p>;

  return (
    <ul>
      {grants?.map((g) => (
        <li key={g.uri}>
          Shared with {g.recipient} on {new Date(g.createdAt).toLocaleDateString()}
        </li>
      ))}
    </ul>
  );
}`;

export const usePendingSharesExample = `import { usePendingShares, useCancelPendingShare } from "@opake/react";

function PendingList() {
  const { data: pending, isLoading } = usePendingShares();
  const cancel = useCancelPendingShare();
  if (isLoading) return null;

  return (
    <ul>
      {pending?.map((p) => (
        <li key={p.uri}>
          {p.document} \u2192 {p.recipient}{" "}
          <button onClick={() => cancel.mutate(p.uri)} disabled={cancel.isPending}>
            Cancel
          </button>
        </li>
      ))}
    </ul>
  );
}`;

// -- react/mutations.mdx ----------------------------------------------------

export const useUploadExample = `import { useUpload } from "@opake/react";

function UploadButton({ keyringUri, directoryUri }: {
  keyringUri: string | null; // null for cabinet
  directoryUri: string;
}) {
  const upload = useUpload(keyringUri);

  const onFile = async (file: File) => {
    const data = new Uint8Array(await file.arrayBuffer());
    await upload.mutateAsync({
      data,
      filename: file.name,
      mimeType: file.type || "application/octet-stream",
      directoryUri,
    });
    // A placeholder row appears in any useDirectory watching
    // directoryUri immediately via the optimistic overlay; it's
    // replaced by the real entry once the SSE echo arrives.
  };

  return <input type="file" onChange={(e) => onFile(e.target.files![0]!)} />;
}`;

export const useDownloadExample = `import { useDownload } from "@opake/react";

function DownloadButton({ keyringUri, documentUri }: {
  keyringUri: string | null;
  documentUri: string;
}) {
  const download = useDownload(keyringUri);

  const onClick = async () => {
    const { filename, data } = await download.mutateAsync(documentUri);
    const url = URL.createObjectURL(new Blob([data]));
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <button onClick={onClick} disabled={download.isPending}>
      {download.isPending ? "Downloading\u2026" : "Download"}
    </button>
  );
}`;

export const useDeleteExample = `import { useDelete } from "@opake/react";

function DeleteButton({ keyringUri, documentUri, parentDirectoryUri }: {
  keyringUri: string | null;
  documentUri: string;
  parentDirectoryUri: string;
}) {
  const del = useDelete(keyringUri);

  return (
    <button
      onClick={() => del.mutate({ documentUri, parentDirectoryUri })}
      disabled={del.isPending}
    >
      {del.isPending ? "Deleting\u2026" : "Delete"}
    </button>
  );
}`;

export const useMoveExample = `import { useMove } from "@opake/react";

function useDragDropMove(keyringUri: string | null) {
  const move = useMove(keyringUri);

  return (entry: { uri: string; parentUri: string }, targetDirUri: string) => {
    move.mutate({
      entryUri: entry.uri,
      sourceDirUri: entry.parentUri,
      targetDirUri,
    });
    // The overlay shows the entry in its new directory immediately;
    // the real tree update arrives via SSE and dedups against the overlay.
  };
}`;

export const useDirectoryMutationsExample = `import {
  useCreateDirectory,
  useRenameDirectory,
  useDeleteDirectory,
} from "@opake/react";

function DirectoryActions({ keyringUri, parentUri }: {
  keyringUri: string | null;
  parentUri: string;
}) {
  const create = useCreateDirectory(keyringUri);
  const rename = useRenameDirectory(keyringUri);
  const remove = useDeleteDirectory(keyringUri);

  return (
    <>
      <button onClick={() => create.mutate({ name: "New Folder", parentUri })}>
        New folder
      </button>
      <button
        onClick={() =>
          rename.mutate({ directoryUri: parentUri, newName: "Renamed" })
        }
      >
        Rename
      </button>
      <button onClick={() => remove.mutate({ directoryUri: parentUri })}>
        Delete folder
      </button>
    </>
  );
}`;

export const useShareMutationsExample = `import { OpakeError } from "@opake/sdk";
import { useShareFile, useRevokeShare } from "@opake/react";

function ShareButton({ documentUri, handle }: {
  documentUri: string;
  handle: string;
}) {
  const share = useShareFile();

  const onClick = async () => {
    const result = await share.mutateAsync({
      documentUri,
      handleOrDid: handle,
      note: "Here's that file you asked for",
    });
    // result.pending === true means the recipient hasn't published
    // an encryption key yet; the daemon retries until they do.
    notify(result.pending ? "Queued until recipient is ready" : "Shared");
  };

  return (
    <button onClick={onClick} disabled={share.isPending}>
      Share with {handle}
    </button>
  );
}

function RevokeButton({ grantUri }: { grantUri: string }) {
  const revoke = useRevokeShare();
  return (
    <button onClick={() => revoke.mutate(grantUri)} disabled={revoke.isPending}>
      Revoke
    </button>
  );
}`;

export const useCreateWorkspaceExample = `import { useState } from "react";
import { useCreateWorkspace } from "@opake/react";

function NewWorkspaceForm() {
  const create = useCreateWorkspace();
  const [name, setName] = useState("");

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        create.mutate({ name });
        // The new workspace appears in useWorkspaces() automatically
        // via the SSE keyring:upsert echo. No cache invalidation needed.
      }}
    >
      <input value={name} onChange={(e) => setName(e.target.value)} />
      <button disabled={!name || create.isPending}>Create workspace</button>
    </form>
  );
}`;

export const overlayMechanics = `// You don't interact with the overlay directly — mutation hooks
// apply patches in onMutate and release them 2s after settle. The
// timeline, for a useMove:
//
//   t+0      move.mutate({...}) fires
//   t+0      overlay patch applied  \u2192 useDirectory re-renders with
//            the entry in its new directory
//   t+~150ms PDS write completes    \u2192 mutation resolves
//   t+~1s    SSE echo arrives       \u2192 TreeKeeper updates, base
//            snapshot reflects the move
//   t+2s     overlay patch released \u2192 no-op because the base
//            snapshot already agrees
//
// If the mutation fails (network hiccup, auth expired, permission
// denied), the overlay releases immediately in onError and the UI
// snaps back to the pre-mutation tree.`;

// -- react/live-updates.mdx -------------------------------------------------

export const useStartSseGated = `import { useStartSseConsumer } from "@opake/react";

function SseGate({ children }: { children: ReactNode }) {
  const user = useAuthUser();
  // Skip starting when there's no authenticated user. Starting anyway
  // would trigger a token exchange that fails with Auth.
  useStartSseConsumer(user ? undefined : null);
  return <>{children}</>;
}`;

export const useStartSseOverride = `// Override the indexer URL at runtime. Wins over the user's PDS
// accountConfig for the rest of the Opake instance's lifetime.
useStartSseConsumer("https://indexer.example.com");`;

export const useDaemonExample = `import { useDaemon } from "@opake/react";
import { Opake } from "@opake/sdk";

function DaemonRunner({ taskStore }: { taskStore: TaskStore }) {
  // Side-effect hook: starts the daemon on mount, stops it on unmount.
  // Returns void \u2014 task state lives in your TaskStore, not in React.
  useDaemon({
    taskDefs: Opake.taskDefs(),
    taskStore,
    onSessionExpired: () => {
      // Your app's logout path \u2014 the daemon hit a dead session and
      // can't continue without re-auth.
      window.location.href = "/login";
    },
  });

  return null; // or a small status indicator wired to taskStore
}`;

export const manualInvalidation = `import { useQueryClient } from "@tanstack/react-query";
import { opakeKeys } from "@opake/react";

function RefreshButton({ documentUri }: { documentUri: string }) {
  const qc = useQueryClient();
  return (
    <button
      onClick={() => {
        // Most state updates via SSE. These factories are for the few
        // queries that don't (shares, pending shares, tasks) \u2014 hit
        // them if you know an external write landed outside the stream.
        qc.invalidateQueries({ queryKey: opakeKeys.shares(documentUri) });
        qc.invalidateQueries({ queryKey: opakeKeys.pendingShares() });
      }}
    >
      Refresh
    </button>
  );
}`;

export const keyFactoriesShape = `opakeKeys.all();                          // every opake query
opakeKeys.cabinetTree();                  // cabinet directory tree
opakeKeys.workspaceTree(keyringUri);      // one workspace's tree
opakeKeys.metadata(directoryUri);         // decrypted doc metadata for a dir
opakeKeys.tasks();                        // daemon task records
opakeKeys.identity(handleOrDid);          // resolved identity cache
opakeKeys.inbox();                        // incoming shares (legacy \u2014 SSE now)
opakeKeys.sharesAll();                    // every shares query across docs
opakeKeys.shares(documentUri);            // outgoing shares for one doc
opakeKeys.pendingShares();                // queued shares waiting on recipient`;
