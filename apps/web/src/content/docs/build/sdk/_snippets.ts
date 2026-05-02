// Code snippets for the SDK overview and authentication pages.
//
// MDX 3 dedents multi-line template literals that appear inside .mdx files
// (both in JSX children and in attribute positions). Moving the snippets
// into a plain .ts file bypasses the MDX parser — template literals here
// preserve indentation verbatim.

// -- overview.mdx -----------------------------------------------------------

export const helloCabinet = `import { Opake } from "@opake/sdk";
import { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";

const storage = new IndexedDbStorage();

// Assumes the user has already logged in. See authentication.mdx.
const opake = await Opake.init({ storage });
const fm = await opake.cabinet();

// Upload
const bytes = new TextEncoder().encode("hello from an encrypted cabinet");
await fm.uploadAt(bytes, "greetings.txt", "text/plain");

// Read back
const { plaintext, filename } = await fm.downloadAt("/greetings.txt");
console.log(filename, new TextDecoder().decode(plaintext));
// → "greetings.txt hello from an encrypted cabinet"`;

export const staticSurface = `// OAuth two-step
Opake.startLogin(handle, options);
Opake.completeLogin(code, state, pending, options);
Opake.loginWithAppPassword(options);

// Seed-phrase recovery
Opake.generateMnemonic();
Opake.validateSeedPhrase(phrase);
Opake.createIdentity(phrase, did);

// Device pairing
Opake.createPairRequest(storage, did);
Opake.awaitPairCompletion(storage, did, rkey);
Opake.cancelPairRequest(storage, did, rkey);

// The one that returns an Opake instance
Opake.init({ storage, did? });`;

export const instanceSurface = `opake.did; // invariant for the instance's lifetime

// FileManager access
opake.cabinet();
opake.workspaceByUri(uri);

// Workspace lifecycle
opake.createWorkspace(name, description?);
opake.listWorkspaces();
opake.watchWorkspaces(handler);

// Incoming shares
opake.listInbox();
opake.watchInbox(handler);

// Live updates
opake.startSseConsumer(indexerUrl?);
opake.stopSseConsumer();

// ...plus pairing (approving side) and maintenance ops`;

export const storageOptions = `// Browsers. The default choice.
import { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";
const storage = new IndexedDbStorage(); // persists via Dexie

// Tests and scripts. In-memory, lost on process exit.
import { MemoryStorage } from "@opake/sdk";
const storage = new MemoryStorage();`;

export const errorHandling = `import { OpakeError } from "@opake/sdk";

try {
  const opake = await Opake.init({ storage });
} catch (err) {
  if (err instanceof OpakeError) {
    switch (err.kind) {
      case "IdentityMissing":
        // Normal: the user is signed in but hasn't bootstrapped an
        // identity on this device yet. Route them to creation, recovery,
        // or pairing.
        break;
      case "NotFound":
        // No session at all. Send them to login.
        break;
      case "Auth":
        // Token refresh failed. The session is dead.
        break;
      default:
        throw err;
    }
  }
}`;

// -- authentication.mdx -----------------------------------------------------

export const startLogin = `import { Opake } from "@opake/sdk";
import { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";

const storage = new IndexedDbStorage();

const { authUrl, pending } = await Opake.startLogin("alice.bsky.social", {
  redirectUri: "https://myapp.com/callback",
});

Opake.savePendingLogin(pending);
window.location.href = authUrl;`;

export const callbackPage = `import { Opake } from "@opake/sdk";
import { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";

const storage = new IndexedDbStorage();

const pending = Opake.loadPendingLogin();
if (!pending) {
  // No pending state, or expired (10-minute TTL). Send back to login.
  window.location.href = "/login";
}

const params = new URLSearchParams(window.location.search);
await Opake.completeLogin(
  params.get("code")!,
  params.get("state")!,
  pending!,
  { storage, redirectUri: "https://myapp.com/callback" },
);

const opake = await Opake.init({ storage });`;

export const embeddedLogin = `await Opake.login("alice.bsky.social", {
  storage,
  redirectUri: "https://myapp.com/callback",
  authorize: async (authUrl) => {
    const popup = window.open(authUrl, "_blank", "width=600,height=800");
    // Resolve with { code, state } however your host surfaces the callback —
    // postMessage from the popup, main-window listener on custom URL scheme,
    // Electron BrowserWindow webContents event, etc.
    return await waitForCallback(popup);
  },
});`;

export const appPasswordLogin = `import { Opake } from "@opake/sdk";

await Opake.loginWithAppPassword(
  "alice.bsky.social",
  "xxxx-xxxx-xxxx-xxxx",
  { storage },
);

const opake = await Opake.init({ storage });`;

export const routingOnInitErrors = `import { Opake, OpakeError } from "@opake/sdk";

try {
  const opake = await Opake.init({ storage });
  // Signed in, identity loaded. Proceed.
} catch (err) {
  if (err instanceof OpakeError) {
    switch (err.kind) {
      case "NotFound":
        // No session in storage for this DID. Send to login.
        window.location.href = "/login";
        break;
      case "IdentityMissing":
        // Session is live, but no encryption identity yet on this device.
        // Route to identity creation, seed-phrase recovery, or device pairing.
        window.location.href = "/recover-or-pair";
        break;
      case "Auth":
        // Session exists but refresh failed. Credentials are stale.
        await storage.clearSession(accounts[0]!.did);
        window.location.href = "/login";
        break;
      default:
        throw err;
    }
  } else {
    throw err;
  }
}`;

export const accountSwitcher = `const accounts = await Opake.listAccounts(storage);
for (const { did, handle, pdsUrl } of accounts) {
  console.log(handle, did, pdsUrl);
}

// Open a specific account:
const opake = await Opake.init({ storage, did: accounts[0]!.did });`;

// -- identity.mdx -----------------------------------------------------------

export const identityShape = `interface Identity {
  did: string;
  x25519_public_key: string;    // X25519, base64. Classical half of the hybrid KEM.
  x25519_private_key: string;   // X25519, base64. Lives in Storage, read by WASM only.
  ml_kem_public_key: string;    // ML-KEM-768, base64. Post-quantum half.
  ml_kem_private_key: string;   // ML-KEM-768, base64. Lives in Storage, read by WASM only.
  signing_key?: string;         // Ed25519, base64. Lives in Storage, read by WASM only.
  verify_key?: string;          // Ed25519, base64.
}`;

export const createIdentityFresh = `// Generate a new 24-word BIP-39 phrase.
const phrase = await Opake.generateSeedPhrase();

// UX: show \`phrase\` to the user and require confirmation it's written
// down BEFORE you call createIdentity. Once the derived identity is
// saved to Storage, the phrase is the only recovery path if Storage
// is ever wiped.
await requireUserConfirmedBackup(phrase);

// Derive the X25519 + Ed25519 keypairs from the phrase.
const identity = await Opake.createIdentity(phrase, did);
await storage.saveIdentity(did, identity);

// Publish the public key so others can encrypt to you.
const opake = await Opake.init({ storage, did });
await opake.publishPublicKey();`;

export const recoverFromPhrase = `// Validate before deriving. Garbage input would silently produce
// garbage keys that only fail later at decrypt time.
if (!(await Opake.validateSeedPhrase(phrase))) {
  throw new Error("That doesn't look like a valid 24-word phrase.");
}

const identity = await Opake.createIdentity(phrase, did);
await storage.saveIdentity(did, identity);

const opake = await Opake.init({ storage, did });

// publishPublicKey is idempotent. If the user has used Opake before on
// another device, the record is already there and the putRecord
// overwrites with the same content. Safe to call either way.
await opake.publishPublicKey();`;

export const createPairRequestNewDevice = `// New device. User has a live session (from login) but no Identity yet.

const { uri, rkey, ephemeralPublicKey } = await Opake.createPairRequest(
  storage,
  did,
);

// Show a fingerprint so the user can verify on their other device that
// the request really is from this one. First 8 bytes of the ephemeral
// public key, hex-paired.
const fingerprint = Array.from(ephemeralPublicKey.slice(0, 8))
  .map((b) => b.toString(16).padStart(2, "0"))
  .join(":");
displayToUser(\`Fingerprint: \${fingerprint}\`);`;

export const awaitPairCompletionNewDevice = `const controller = new AbortController();

try {
  await Opake.awaitPairCompletion(storage, did, rkey, {
    pollIntervalMs: 3000,
    timeoutMs: 10 * 60 * 1000, // 10 minutes
    signal: controller.signal,
  });

  // Identity is in Storage. Opake.init now succeeds.
  const opake = await Opake.init({ storage, did });
} catch (err) {
  if (err instanceof DOMException && err.name === "AbortError") {
    // User cancelled before approval arrived. Wipe the request.
    await Opake.cancelPairRequest(storage, did, rkey);
    return;
  }
  throw err;
}`;

export const approvePairRequestExistingDevice = `// Existing device. Already has an Identity in Storage, so \`opake\` exists.

const requests = await opake.listPairRequests();
// requests: {
//   uri: string;
//   x25519EphemeralKey: Uint8Array;  // 32 bytes — what fingerprints
//   mlKemEphemeralKey: Uint8Array;   // 1184 bytes — post-quantum half
//   createdAt: string;
// }[]

// Present them with fingerprints (X25519 half). The user picks the one
// matching what their new device is showing. Do NOT auto-approve — the
// fingerprint check is the only defense against a MITM injecting a fake
// request.
const selected = await askUserToPickRequest(requests);

await opake.approvePairRequest(
  selected.uri,
  selected.x25519EphemeralKey,
  selected.mlKemEphemeralKey,
);
// The new device's awaitPairCompletion resolves within one poll.`;

// -- files.mdx --------------------------------------------------------------

export const getFileManager = `// Personal files. One FileManager per Opake instance.
const cabinet = await opake.cabinet();

// Shared workspace. Resolved by URI; the WASM side unwraps the group
// key internally using the caller's Identity, and the resulting
// FileManager routes reads to the owner's PDS, writes to proposals
// if the caller isn't the owner.
const workspace = await opake.workspaceByUri(workspaceUri);`;

export const uploadDocument = `const data = await readAsBytes(userFile); // from your UI

const result = await fm.upload(data, "budget.pdf", "application/pdf", {
  description: "Q4 planning doc",
  tags: ["finance", "q4"],
  directoryUri: currentDirectoryUri, // where to place it in the tree
});

if (result.proposed) {
  // Workspace member: the upload was written as a documentUpdate proposal.
  // The owner's daemon will apply it on their next sync.
  notify("Upload queued — waiting for the workspace owner to apply.");
} else {
  // Cabinet, or a workspace you own: applied immediately.
  notify("Uploaded.");
}`;

export const downloadDocument = `const { filename, data } = await fm.download(documentUri);

// data is a Uint8Array — decrypted plaintext ready for use.
const blob = new Blob([data], { type: "application/octet-stream" });
saveAsFile(filename, blob);`;

export const readingTrees = `// Fast load. Uses whatever's in the local cache plus a delta sync;
// returns the directory structure only, no document metadata.
const tree = await fm.loadTree();

// Same tree, plus decrypted metadata for one directory's documents.
// Use this when rendering a directory view — the extra round-trips
// only happen for that one directory.
const withNames = await fm.loadTreeWithMetadata(currentDirectoryUri);

// Force a fresh PDS fetch before returning. Use sparingly; this bypasses
// the cache. Typically called after a write that invalidates state the
// indexer hasn't caught up to yet.
const fresh = await fm.syncAndLoadTree(currentDirectoryUri);`;

export const structureChanges = `// Create a directory.
await fm.createDirectory("Photos", parentDirectoryUri);

// Move an entry (document or sub-directory) between directories.
await fm.move(entryUri, sourceDirectoryUri, targetDirectoryUri);

// Delete a document. parentDirectoryUri is required: the delete is
// atomic with removing this entry from the parent.
await fm.delete(documentUri, parentDirectoryUri);

// Recursively delete a directory and everything inside it.
await fm.deleteRecursive(directoryUri);`;

export const updateMetadataContent = `// Change metadata without re-uploading the blob.
await fm.updateMetadata(documentUri, {
  filename: "budget-final.pdf",
  description: "Approved version",
  tags: ["finance", "q4", "approved"],
});

// Replace document contents in place. Existing grants stay valid —
// updateContent re-encrypts with the same content key, so recipients
// don't need re-wrapping.
await fm.updateContent(documentUri, newBytes);`;

export const watchDirectory = `const watcher = fm.watchDirectory(directoryUri, (snapshot) => {
  if (snapshot === null) {
    // The directory was deleted (by the owner, or recursively from a
    // parent). Stop reading state that references it.
    handleDirectoryGone();
    return;
  }
  renderDirectory(snapshot);
});

// Stop listening when you're done — on UI teardown, logout, or before
// switching to a different directory.
watcher.close();`;

// -- sharing.mdx ------------------------------------------------------------

export const shareDocument = `// Resolve the recipient first. This returns the recipient's hybrid public-key
// bundle: x25519PublicKey (classical) + mlKemPublicKey (post-quantum), plus
// the algo strings advertised on their app.opake.publicKey/self record.
const recipient = await opake.resolveIdentity("bob.bsky.social");

// Direct share. Writes an app.opake.grant record on YOUR PDS that wraps
// the document's content key to BOTH halves of the recipient's hybrid
// bundle. The grant lives under your repo; the recipient discovers it
// via the indexer.
const fm = await opake.cabinet();
await fm.share(
  documentUri,
  recipient.did,
  recipient.x25519PublicKey,
  recipient.mlKemPublicKey,
  "read",
  "For your review — draft v2",
);`;

export const handleRecipientNotReady = `import { OpakeError } from "@opake/sdk";

try {
  const recipient = await opake.resolveIdentity(handleOrDid);
  await fm.share(
    documentUri,
    recipient.did,
    recipient.x25519PublicKey,
    recipient.mlKemPublicKey,
    "read",
  );
} catch (err) {
  if (err instanceof OpakeError && err.kind === "RecipientNotReady") {
    // The target has a valid atproto identity but hasn't published an
    // Opake public key yet (hasn't used Opake). Queue a pending share;
    // the daemon will retry until they sign up or it expires (7 days).
    await fm.createPendingShare(documentUri, handleOrDid, "read", null);
    return;
  }
  throw err;
}`;

export const listOutgoingShares = `const grants = await fm.listShares();
for (const g of grants) {
  // g: { uri, document, recipient, createdAt, expiresAt }
  console.log(\`Shared \${g.document} with \${g.recipient}\`);
}

// Grants don't carry the document filename directly; metadata stays
// encrypted on the wire. If you need names, do a separate
// getDocumentMetadata lookup per document URI.`;

export const revokeShareSnippet = `await fm.revokeShare(grantUri);
// The grant record is deleted from your PDS. The indexer removes it
// from its index on the next firehose event; the recipient's
// watchInbox fires with the updated (shorter) list.
//
// Caveat: revocation is forward-only. If the recipient already
// decrypted and saved the key, you can't take that back. If the
// document needs to be unreadable to the ex-recipient going forward,
// delete-and-reupload instead of revoke.`;

export const listInboxSnippet = `const grants = await opake.listInbox();
for (const g of grants) {
  // g: { uri, ownerDid, documentUri, createdAt }
  // ownerDid is the DID of whoever shared with you.
  console.log(\`Shared with you: \${g.documentUri} from \${g.ownerDid}\`);
}`;

export const watchInbox = `const watcher = opake.watchInbox((snapshot) => {
  // snapshot.loaded is false while the keeper is bootstrapping. Once
  // the initial listInbox completes, it flips to true and fires again
  // with the current entries.
  renderInbox(snapshot.entries, snapshot.loaded);
});

// Stop listening when you're done.
watcher.close();`;

export const downloadFromGrantSnippet = `// Peek at the metadata (filename, MIME type, size, etc.) without
// downloading the blob. Useful for rendering a "shared with me" list.
const { filename, metadata } = await opake.resolveGrantMetadata(grantUri);

// Download and decrypt in full.
const { filename: f, data } = await opake.downloadFromGrant(grantUri);
saveAsFile(f, new Blob([data]));`;

export const pendingShares = `// List queued shares that haven't landed as grants yet.
const pending = await opake.listPendingShares();
for (const p of pending) {
  // p: { uri, document, recipient, createdAt }
  console.log(\`Pending: \${p.document} → \${p.recipient}\`);
}

// Manually kick the retry loop. The daemon runs this on a schedule too.
const outcome = await opake.retryPendingShares();
// { checked, completed, expired, still_pending, failed }

// Cancel a specific pending share.
await opake.cancelPendingShare(pendingShareUri);`;

// -- workspaces.mdx ---------------------------------------------------------

export const createWorkspace = `const { keyringUri, key } = await opake.createWorkspace(
  "Family Photos",
  "Shared vacation + milestone photos",
);

// key is the group key for this workspace. You don't usually hold onto
// it; subsequent operations re-resolve via keyringUri and the caller's
// Identity unwraps the member entry each time.
const fm = await opake.workspaceByUri(keyringUri);`;

export const listAndWatchWorkspaces = `// One-shot list.
const workspaces = await opake.listWorkspaces();
for (const ws of workspaces) {
  console.log(ws.name, ws.role, ws.keyringUri);
}

// Subscription. Fires once immediately with the current snapshot,
// then again on every keyring:upsert / keyring:delete SSE event.
const watcher = opake.watchWorkspaces((snapshot) => {
  renderWorkspaceList(snapshot.entries, snapshot.loaded);
});

// Stop listening when you're done.
watcher.close();`;

export const addWorkspaceMember = `// Resolve the invitee's DID once for UI affordances, then hand it off.
// Core handles the rest — resolves the hybrid public-key bundle inside
// WASM, wraps the group key to it, writes the updated keyring record.
const recipient = await opake.resolveIdentity(handle);

await opake.addWorkspaceMember(keyringUri, recipient.did, "editor");

// The workspace keyring record on the owner's PDS now has an extra
// member entry containing the group key wrapped to the invitee's
// hybrid bundle. They'll see the workspace in their next listWorkspaces.`;

export const removeWorkspaceMember = `// Owner removing a member rotates the group key in place.
const result = await opake.removeWorkspaceMember(keyringUri, memberDid);

if (result.proposed) {
  // The caller isn't the owner; this was written as a keyringUpdate
  // proposal. The owner's daemon applies it.
  return;
}

// Owner path: result.rotation is the new rotation number. Existing
// documents stay readable by remaining members because keyHistory
// retains the prior rotation's member entries. Documents uploaded
// AFTER this point are wrapped under the new key, which the removed
// member doesn't have.
console.log("Rotated to", result.rotation);`;

export const leaveWorkspace = `// Opt out of a workspace you're a member of (not the owner of).
// Writes a keyringUpdate proposal with actionType "leave"; the owner's
// daemon processes it and triggers a normal remove-member rotation.
await opake.leaveWorkspace(keyringUri);`;

export const proposalFlow = `// Member writing to a workspace they don't own.
const result = await fm.upload(data, "notes.md", "text/markdown", {
  directoryUri: someWorkspaceDir,
});
// result.uri points at a documentUpdate proposal record on the CALLER's
// PDS, NOT at a new document on the owner's PDS.
// result.proposed === true

// Owner side (running anywhere the owner's Opake instance is alive):
// syncWorkspaceByUri picks up pending proposals, validates the
// proposer's role, applies them as canonical records on the owner's
// PDS, and deletes the proposal from the member's PDS.
await opake.syncWorkspaceByUri(keyringUri);`;

export const mutationResultHandling = `// Every write returns MutationResult:
//   { uri: string; proposed: boolean }
//
// proposed: true means the write is a documentUpdate/directoryUpdate
// record on the caller's own PDS, waiting for the workspace owner to
// apply it. The URI in that case points at the proposal record, not the
// target document/directory, so don't treat it as a new document URI.
//
// proposed: false means the write was applied directly — either the
// caller owns the workspace (or it's their cabinet), or the owner is
// acting on their own records.

const result = await fm.updateMetadata(documentUri, { filename: "v2.pdf" });
if (result.proposed) {
  // Optimistic UI: mark the row as "pending apply" but show the new name.
  markPending(documentUri, "v2.pdf");
} else {
  // Direct apply: the change is already on PDS.
  updateLocalTree(documentUri, "v2.pdf");
}`;

// -- events.mdx -------------------------------------------------------------

export const startConsumerBasic = `// Start the stream with the indexer URL resolved via the priority chain
// (runtime override → user's accountConfig on PDS → compile-time default).
// Idempotent: a second call while one is running is a no-op.
await opake.startSseConsumer();`;

export const startConsumerCustom = `// Override the indexer URL at runtime. Wins over whatever's in the user's
// accountConfig for the rest of the Opake instance's lifetime.
await opake.startSseConsumer("https://indexer.example.com");`;

export const teardownSequence = `// Logout / account switch teardown. Do it in this order.

// 1. Stop the stream so no events land against state you're about to drop.
opake.stopSseConsumer();

// 2. Drain the in-memory keepers. ContentKeys are zeroized; the decrypted
//    directory-name cache is cleared. Anything sitting on an \`opake.watch*\`
//    handler receives one final snapshot (empty, loaded=false) and closes.
opake.wipeState();

// OpakeProvider in @opake/react does this pair on unmount for you.`;

export const consumerGating = `// Hold off on starting the stream until the app has a live session.
// Typical: inside a useEffect / onMount that depends on auth state.

if (session.status === "active") {
  await opake.startSseConsumer();
}

// If the user hasn't logged in yet, starting anyway would trigger a
// token-exchange request that fails with Auth. Cleaner to gate on
// authentication state.`;

// -- storage.mdx ------------------------------------------------------------

export const storageInterface = `interface Storage {
  // Config — account roster and default DID.
  loadConfig(): Promise<Config>;
  saveConfig(config: Config): Promise<void>;

  // Identity — X25519 + Ed25519 keypairs, per DID.
  loadIdentity(did: string): Promise<Identity>;
  saveIdentity(did: string, identity: Identity): Promise<void>;

  // Session — OAuth or legacy tokens, DPoP keys, per DID.
  loadSession(did: string): Promise<Session>;
  saveSession(did: string, session: Session): Promise<void>;
  clearSession(did: string): Promise<void>;

  // Full account removal (identity + session + cache + config entry).
  removeAccount(did: string): Promise<void>;

  // Ephemeral pair-state: raw bytes of the X25519 private half during
  // pairing. WASM writes on createPairRequest, reads on tryCompletePair,
  // deletes on success or cancel. Never crosses back into JS.
  savePairState(did: string, rkey: string, privateKey: Uint8Array): Promise<void>;
  loadPairState(did: string, rkey: string): Promise<Uint8Array>;
  deletePairState(did: string, rkey: string): Promise<void>;

  // PDS record cache. Not secret; the same ciphertext the PDS would
  // serve. Used for fast cold starts and offline reads.
  cacheGetRecord<T>(did, collection, uri): Promise<CachedRecord<T> | null>;
  cachePutRecords<T>(did, collection, records): Promise<void>;
  cacheRemoveRecord(did, collection, uri): Promise<void>;
  cacheGetCollection<T>(did, collection): Promise<CachedCollection<T> | null>;
  cachePutCollection<T>(did, collection, data): Promise<void>;
  cacheInvalidateCollection(did, collection): Promise<void>;
  cacheClear(did): Promise<void>;
}`;

export const storageBuiltIns = `import { IndexedDbStorage } from "@opake/sdk/storage/indexeddb";
import { MemoryStorage } from "@opake/sdk";

// Browsers. Backed by Dexie; survives page reloads; scoped per origin.
const browserStorage = new IndexedDbStorage();

// Tests. All in-memory, discarded on process exit.
const testStorage = new MemoryStorage();`;

export const storageCustomBackend = `import type { Storage, Config, Identity, Session } from "@opake/sdk";

class TauriFsStorage implements Storage {
  constructor(private readonly dataDir: string) {}

  async loadConfig(): Promise<Config> {
    const raw = await fs.readFile(\`\${this.dataDir}/config.json\`);
    return JSON.parse(raw.toString()) as Config;
  }

  async saveConfig(config: Config): Promise<void> {
    await fs.writeFile(
      \`\${this.dataDir}/config.json\`,
      JSON.stringify(config, null, 2),
    );
  }

  // ...identity, session, pair-state, cache methods follow the same
  // pattern. Reference: MemoryStorage (src/storage/memory.ts) is a
  // clean example of the full interface in one file.
}`;
