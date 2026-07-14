// Stateful WASM exports for Opake domain types.
//
// JS callers construct an OpakeContext, then either:
// - Call workspace management methods directly (createWorkspace, listWorkspaces, etc.)
// - Call .cabinet() or .workspaceByUri() to get a FileManager for file operations
//
// FileManager shares access via Rc<Mutex<>>. Both OpakeContext and
// FileManager hold Rc clones of the same WasmOpake — the Mutex serializes
// concurrent async operations.
//
// The inner Mutex (futures_util::lock::Mutex) replaces RefCell. RefCell
// panics when borrowed concurrently across async boundaries (the JS event
// loop can interleave WASM calls while a future is suspended at a network
// await). The async Mutex queues instead of panicking — second caller
// waits until the first finishes. In WASM's single-threaded context there
// is no OS-level locking, just a flag + waker.
//
// All wasm_bindgen methods take &self (not &mut self) because the Mutex
// provides interior mutability. Using &mut self on async wasm_bindgen
// methods triggers wasm-bindgen's internal borrow tracking, which panics
// when two &mut self async operations overlap across await points.

use std::rc::Rc;

use futures_util::lock::Mutex;
use opake_core::indexer::chain_fork_keeper::ChainForkKeeper;
use opake_core::indexer::inbox_keeper::InboxKeeper;
use opake_core::indexer::tree_keeper::TreeKeeper;
use opake_core::indexer::workspace_keeper::WorkspaceKeeper;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::file_manager_wasm::WasmFileManagerHandle;
use crate::js_storage::JsStorageAdapter;
use crate::wasm_util::{
    cabinet_context, make_opake_from_storage, parse_role, pub_key_from_slice, to_js, wasm_err,
    DownloadResult, MutationResultDto, WasmOpake,
};

// ---------------------------------------------------------------------------
// OpakeContext
// ---------------------------------------------------------------------------

#[wasm_bindgen(js_name = OpakeContext)]
pub struct WasmOpakeHandle {
    pub(crate) inner: Rc<Mutex<WasmOpake>>,
    /// Persistent tree state + SSE watcher registry. Held behind its own
    /// Mutex (separate from the Opake mutex) so SSE event application and
    /// file operations don't block each other.
    pub(crate) tree_keeper: Rc<Mutex<TreeKeeper>>,
    /// Live workspace-list state. Bootstrapped from `listWorkspaces` and
    /// patched incrementally from SSE `keyring:upsert` / `keyring:delete`
    /// events. JS subscribes via `watchWorkspaces`. Separate mutex from
    /// `tree_keeper` so directory watcher fires and workspace list fires
    /// don't block each other.
    pub(crate) workspace_keeper: Rc<Mutex<WorkspaceKeeper>>,
    /// Live inbox state. Bootstrapped from `listInbox` and patched
    /// incrementally from SSE `grant:upsert` / `grant:delete` events.
    /// JS subscribes via `watchInbox`.
    pub(crate) inbox_keeper: Rc<Mutex<InboxKeeper>>,
    /// Pub-sub for `chain:forked` SSE events. Holds no state; just fans
    /// dispatch out to subscribers (typically the React mutation-retry
    /// layer). JS subscribes via `watchChainForks`.
    pub(crate) chain_fork_keeper: Rc<Mutex<ChainForkKeeper>>,
    /// `true` while an SSE consumer task is alive — the idempotency gate
    /// on `startSseConsumer` (a second start while one runs is a no-op).
    pub(crate) sse_running: Rc<std::cell::Cell<bool>>,
    /// Monotonic consumer-generation token. Each spawned consumer task
    /// captures the generation at spawn; it stays the live consumer only
    /// while `sse_generation == its captured value`. `stopSseConsumer`
    /// bumps the generation, so a task still blocked in
    /// `next_event().await` learns it was superseded the moment it
    /// resumes — even if a *new* consumer has since started and flipped
    /// `sse_running` back to `true`. A plain bool can't tell "stop me"
    /// apart from "a newer consumer is now running"; the generation can.
    /// Single-threaded WASM — `Cell` is enough, no atomics.
    pub(crate) sse_generation: Rc<std::cell::Cell<u64>>,
    /// Buffers keyring events that land while a `listWorkspaces` snapshot
    /// fetch is in flight, so the snapshot's wholesale replace can't drop
    /// a concurrent delta. Client-sync policy — the keeper is a dumb
    /// projection and leaves sequencing to us. See [`crate::bootstrap_gate::BootstrapGate`].
    pub(crate) ws_gate: Rc<std::cell::RefCell<crate::bootstrap_gate::BootstrapGate>>,
    /// Same, for grant events during a `listInbox` snapshot fetch.
    pub(crate) inbox_gate: Rc<std::cell::RefCell<crate::bootstrap_gate::BootstrapGate>>,
}

#[wasm_bindgen(js_class = OpakeContext)]
impl WasmOpakeHandle {
    /// Create an OpakeContext from Storage.
    ///
    /// Reads config, session, and identity from the JS-side IndexedDB via
    /// the provided storage adapter. Pass `None` for the default account.
    pub async fn create(
        did: Option<String>,
        storage_adapter: JsStorageAdapter,
    ) -> Result<WasmOpakeHandle, JsError> {
        let opake = make_opake_from_storage(did.as_deref(), storage_adapter).await?;
        let did_owned = opake.did().to_string();
        Ok(Self {
            inner: Rc::new(Mutex::new(opake)),
            tree_keeper: Rc::new(Mutex::new(TreeKeeper::new(did_owned))),
            workspace_keeper: Rc::new(Mutex::new(WorkspaceKeeper::new())),
            inbox_keeper: Rc::new(Mutex::new(InboxKeeper::new())),
            chain_fork_keeper: Rc::new(Mutex::new(ChainForkKeeper::new())),
            sse_running: Rc::new(std::cell::Cell::new(false)),
            sse_generation: Rc::new(std::cell::Cell::new(0)),
            ws_gate: Rc::new(std::cell::RefCell::new(
                crate::bootstrap_gate::BootstrapGate::new(),
            )),
            inbox_gate: Rc::new(std::cell::RefCell::new(
                crate::bootstrap_gate::BootstrapGate::new(),
            )),
        })
    }

    /// Create a cabinet FileManager. Non-consuming — the OpakeContext
    /// remains usable after the FileManager is freed.
    pub async fn cabinet(&self) -> Result<WasmFileManagerHandle, JsError> {
        let context = {
            let guard = self.inner.lock().await;
            cabinet_context(&guard)?
        };

        Ok(WasmFileManagerHandle {
            opake: Rc::clone(&self.inner),
            tree_keeper: Rc::clone(&self.tree_keeper),
            context,
        })
    }

    /// Resolve a workspace by URI and create a FileManager.
    ///
    /// Fetches the keyring, unwraps the group key using the caller's identity,
    /// and returns a ready-to-use FileManager. The key never crosses the
    /// WASM/JS boundary.
    #[wasm_bindgen(js_name = workspaceByUri)]
    pub async fn workspace_by_uri(
        &self,
        keyring_uri: &str,
    ) -> Result<WasmFileManagerHandle, JsError> {
        let mut opake = self.opake().await?;
        let ws = opake
            .resolve_workspace_by_uri(keyring_uri)
            .await
            .map_err(wasm_err)?;
        let context = opake_core::manager::FileContext::Workspace(ws);
        drop(opake);

        Ok(WasmFileManagerHandle {
            opake: Rc::clone(&self.inner),
            tree_keeper: Rc::clone(&self.tree_keeper),
            context,
        })
    }

    // -- Workspace management (does NOT consume the context) --

    /// List members of a workspace. Fetches the keyring record from the
    /// authority's own PDS and returns the member list with DIDs and roles.
    #[wasm_bindgen(js_name = listWorkspaceMembers)]
    pub async fn list_workspace_members(&self, keyring_uri: &str) -> Result<JsValue, JsError> {
        let opake = self.opake().await?;
        let members = opake.workspace_members(keyring_uri).await.map_err(wasm_err)?;
        to_js(&members)
    }

    #[wasm_bindgen(js_name = createWorkspace)]
    pub async fn create_workspace(
        &self,
        name: &str,
        description: Option<String>,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let created = opake
            .create_workspace(name, description.as_deref())
            .await
            .map_err(wasm_err)?;
        drop(opake);

        // No optimistic keeper insert: the workspace projection is patched
        // only by indexer-derived inputs (snapshot + SSE echo), so a sidebar
        // entry appears exactly when the indexer can already answer for it —
        // visibility and actionability become the same event, and a fresh
        // entry is never one that 403s on first use. The create flow signals
        // in-flight state until the `keyring:upsert` echo delivers the entry
        // (see the web create dialog); the returned URI lets the caller await
        // that arrival rather than fabricate a provisional row here.
        to_js(&crate::bindings::CreateWorkspaceResultDto {
            keyring_uri: created.keyring_uri,
            key: created.key.0.to_vec(),
        })
    }

    /// List all keyrings the user is a member of, with decrypted metadata.
    ///
    /// Side effect: bootstraps the shared `WorkspaceKeeper` with the
    /// result. Any `watchWorkspaces` callers (current or future) receive
    /// a fresh snapshot with `loaded = true` as part of this call. Once
    /// bootstrapped, incremental SSE events keep the keeper in sync
    /// without further `listWorkspaces` round-trips.
    #[wasm_bindgen(js_name = listWorkspaces)]
    pub async fn list_workspaces(&self) -> Result<JsValue, JsError> {
        // Gate-protected bootstrap: the gate buffers keyring events that
        // arrive during the fetch so the snapshot's wholesale replace
        // can't clobber them. Shared with the reconnect resync path.
        let entries = crate::sse_wasm::bootstrap_workspace_keeper(
            &self.inner,
            &self.workspace_keeper,
            &self.ws_gate,
            &self.sse_generation,
        )
        .await?;

        // JS-side wire format — `ListWorkspacesResultDto` is the named
        // shape; the SDK consumes the generated TS type.
        to_js(&crate::bindings::ListWorkspacesResultDto {
            workspaces: entries
                .iter()
                .map(crate::bindings::WorkspaceEntryDto::from)
                .collect(),
        })
    }

    /// Add a member to a workspace. Resolves both the keyring's group key
    /// and the new member's hybrid public-key bundle internally, so neither
    /// the group key nor recipient pubkeys cross the WASM/JS boundary as
    /// loose byte arrays.
    #[wasm_bindgen(js_name = addWorkspaceMember)]
    pub async fn add_workspace_member(
        &self,
        keyring_uri: &str,
        member_did: &str,
        role: &str,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let ws = opake
            .resolve_workspace_by_uri(keyring_uri)
            .await
            .map_err(wasm_err)?;
        let role = parse_role(role)?;
        let _outcome = opake
            .add_workspace_member(&ws.id(), &ws.key, &ws.historical_keys, member_did, role)
            .await
            .map_err(wasm_err)?;
        to_js(&MutationResultDto { uri: None })
    }

    /// Leave a workspace. Resolves the head URI to the stable genesis id
    /// first — `leave_workspace` is a genesis-keyed core operation, and a
    /// JS-supplied URI is always the head (see workspace-identity spec,
    /// "the WASM boundary resolves to genesis before core operations").
    #[wasm_bindgen(js_name = leaveWorkspace)]
    pub async fn leave_workspace(&self, keyring_uri: &str) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let ws = opake
            .resolve_workspace_by_uri(keyring_uri)
            .await
            .map_err(wasm_err)?;
        let _outcome = opake.leave_workspace(&ws.id()).await.map_err(wasm_err)?;
        to_js(&MutationResultDto { uri: None })
    }

    /// Remove a member from a workspace. Resolves the keyring + group key
    /// internally so the key never crosses the WASM/JS boundary.
    ///
    /// Federation cascade: rotates the group key inside WASM, re-wraps for
    /// remaining members, writes a keyring supersede on the caller's PDS.
    /// The rotated key bytes never cross the boundary (they'd be a security
    /// violation — JS can't zeroize). Returns the new rotation index; the
    /// caller re-resolves via `resolve_workspace_by_uri` for the next op.
    #[wasm_bindgen(js_name = removeWorkspaceMember)]
    pub async fn remove_workspace_member(
        &self,
        keyring_uri: &str,
        member_did: &str,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let ws = opake
            .resolve_workspace_by_uri(keyring_uri)
            .await
            .map_err(wasm_err)?;
        let (_new_key, rotation) = opake
            .remove_workspace_member(&ws.id(), &ws.key, member_did)
            .await
            .map_err(wasm_err)?;

        #[derive(Serialize)]
        struct R {
            rotation: u64,
        }
        serde_wasm_bindgen::to_value(&R { rotation }).map_err(|e| JsError::new(&e.to_string()))
    }

    /// Update workspace metadata (name, description, icon). Resolves the
    /// keyring + group key internally so the key never crosses the WASM/JS
    /// boundary. Writes a keyring supersede on the caller's PDS.
    #[wasm_bindgen(js_name = updateWorkspaceMetadata)]
    pub async fn update_workspace_metadata(
        &self,
        keyring_uri: &str,
        name: Option<String>,
        description: Option<String>,
        icon: Option<String>,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let ws = opake
            .resolve_workspace_by_uri(keyring_uri)
            .await
            .map_err(wasm_err)?;
        let _outcome = opake
            .update_workspace_metadata(
                &ws.id(),
                &ws.key,
                name.as_deref(),
                description.as_deref(),
                icon.as_deref(),
            )
            .await
            .map_err(wasm_err)?;
        to_js(&MutationResultDto { uri: None })
    }

    /// Update a workspace member's role. Writes a keyring supersede on the
    /// caller's PDS — manager-authority required at the indexer.
    #[wasm_bindgen(js_name = updateMemberRole)]
    pub async fn update_member_role(
        &self,
        keyring_uri: &str,
        member_did: &str,
        role: &str,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let role = parse_role(role)?;
        // The web hands us the chain-head URI; the core role-change op keys
        // the indexer chain-head lookup and the keyring wrap anchor on the
        // stable genesis URI. `resolve_workspace_by_uri` walks head→genesis
        // via `wrap_anchor`, so `ws.uri` is the genesis URI the op expects.
        let ws = opake
            .resolve_workspace_by_uri(keyring_uri)
            .await
            .map_err(wasm_err)?;
        let _outcome = opake
            .update_member_role(&ws.id(), member_did, role)
            .await
            .map_err(wasm_err)?;
        to_js(&MutationResultDto { uri: None })
    }

    /// Download and decrypt a file using a grant (cross-PDS, recipient side).
    #[wasm_bindgen(js_name = downloadFromGrant)]
    pub async fn download_from_grant(&self, grant_uri: &str) -> Result<JsValue, JsError> {
        let opake = self.opake().await?;
        let (filename, plaintext) = opake
            .download_from_grant(grant_uri)
            .await
            .map_err(wasm_err)?;
        to_js(&DownloadResult {
            filename,
            plaintext,
        })
    }

    /// Approve a pair request from an already-authenticated device. Wraps
    /// this device's identity to the requester's ephemeral hybrid public-key
    /// bundle and publishes the response record.
    #[wasm_bindgen(js_name = approvePairRequest)]
    pub async fn approve_pair_request(
        &self,
        request_uri: &str,
        ephemeral_x25519_public_key: &[u8],
        ephemeral_ml_kem_public_key: &[u8],
    ) -> Result<(), JsError> {
        let x25519_pubkey = pub_key_from_slice(ephemeral_x25519_public_key)?;
        let ml_kem_pubkey: opake_core::crypto::MlKemPublicKey = ephemeral_ml_kem_public_key
            .try_into()
            .map_err(|_| JsError::new("ML-KEM-768 ephemeral key must be 1184 bytes"))?;
        let mut opake = self.opake().await?;
        opake
            .approve_pair_request(request_uri, &x25519_pubkey, &ml_kem_pubkey)
            .await
            .map_err(wasm_err)
    }

    /// Sync a single workspace by keyring URI. Errors if the URI doesn't
    /// resolve to a workspace the caller is a member of.
    ///
    /// Resolves first: core `sync_workspace_by_uri` matches against the
    /// derived genesis id, so a head URI (what JS holds after a supersede)
    /// would silently miss without this (workspace-identity spec,
    /// "sync-by-URI accepts what its caller holds").
    #[wasm_bindgen(js_name = syncWorkspaceByUri)]
    pub async fn sync_workspace_by_uri(&self, keyring_uri: &str) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let ws = opake
            .resolve_workspace_by_uri(keyring_uri)
            .await
            .map_err(wasm_err)?;
        let result = opake
            .sync_workspace_by_uri(&ws.id())
            .await
            .map_err(wasm_err)?;
        to_js(&result)
    }

    /// List pending (queued) outgoing shares on the caller's PDS.
    #[wasm_bindgen(js_name = listPendingShares)]
    pub async fn list_pending_shares(&self) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let entries = opake.list_pending_shares().await.map_err(wasm_err)?;

        #[derive(Serialize)]
        struct Entry {
            uri: String,
            document: String,
            recipient: String,
            created_at: String,
        }

        let out: Vec<Entry> = entries
            .into_iter()
            .map(|e| Entry {
                uri: e.uri,
                document: e.document,
                recipient: e.recipient,
                created_at: e.created_at,
            })
            .collect();
        to_js(&out)
    }

    /// Cancel a pending share by AT-URI.
    #[wasm_bindgen(js_name = cancelPendingShare)]
    pub async fn cancel_pending_share(&self, uri: &str) -> Result<(), JsError> {
        let mut opake = self.opake().await?;
        opake.cancel_pending_share(uri).await.map_err(wasm_err)
    }

    /// Retry all pending shares (resolve recipients, create grants).
    #[wasm_bindgen(js_name = retryPendingSharesViaOpake)]
    pub async fn retry_pending_shares_via_opake(&self) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let resolver = opake_core::client::WasmTransport::new();
        let result = opake
            .retry_pending_shares(&resolver)
            .await
            .map_err(wasm_err)?;
        to_js(&serde_json::json!({
            "checked": result.checked,
            "completed": result.completed,
            "expired": result.expired,
            "still_pending": result.still_pending,
            "failed": result.failed,
        }))
    }

    /// Re-wrap the caller's documents from historical group keys to the
    /// current rotation. Opportunistic background hygiene — exposes the
    /// operation only; no key material crosses to JS.
    #[wasm_bindgen(js_name = sweepRotationRewrap)]
    pub async fn sweep_rotation_rewrap(&self) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let outcome = opake
            .sweep_owned_documents_rewrap()
            .await
            .map_err(wasm_err)?;
        to_js(&serde_json::json!({
            "rewrapped": outcome.rewrapped,
            "already_current": outcome.already_current,
            "conflicts": outcome.conflicts,
        }))
    }

    /// Override the cached indexer URL at runtime.
    ///
    /// Overrides the compile-time `DEFAULT_INDEXER_URL` seeded during
    /// `for_account`. Callers use this at boot to inject a host-specific
    /// runtime default (e.g. web's `VITE_INDEXER_URL`, which can't be
    /// baked in because one WASM binary serves multiple deployments).
    ///
    /// Subsequent writes to `accountConfig` on the PDS still override
    /// this value via `set_account_config` — so a user-configured
    /// indexer (written via settings) wins over the host default.
    #[wasm_bindgen(js_name = setIndexerUrl)]
    pub async fn set_indexer_url(&self, url: String) {
        self.inner.lock().await.set_indexer_url(url);
    }

    /// Verify the session is usable by touching the account config record.
    ///
    /// Reads the config, stamps `modifiedAt`, and writes it back. Throws on
    /// auth failure — the SDK uses this during boot to detect dead sessions.
    #[wasm_bindgen(js_name = checkSession)]
    pub async fn check_session(&self) -> Result<(), JsError> {
        let mut opake = self.opake().await?;
        let now = opake.now();
        let config = opake.get_account_config().await.map_err(wasm_err)?;
        let mut record =
            config.unwrap_or_else(|| opake_core::records::AccountConfigRecord::new(&now));
        record.modified_at = now;
        opake.set_account_config(&record).await.map_err(wasm_err)?;
        Ok(())
    }

    /// Fetch the account config record, if it exists.
    #[wasm_bindgen(js_name = getAccountConfig)]
    pub async fn get_account_config(&self) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let config = opake.get_account_config().await.map_err(wasm_err)?;
        to_js(&config)
    }

    /// Write the account config record (upsert).
    #[wasm_bindgen(js_name = setAccountConfig)]
    pub async fn set_account_config(&self, config_js: JsValue) -> Result<String, JsError> {
        let config: opake_core::records::AccountConfigRecord =
            serde_wasm_bindgen::from_value(config_js).map_err(|e| JsError::new(&e.to_string()))?;
        let mut opake = self.opake().await?;
        opake.set_account_config(&config).await.map_err(wasm_err)
    }

    /// Read-merge-write the account config atomically.
    ///
    /// Accepts a partial updates object — fields omitted (`undefined`)
    /// are left untouched, fields set to `null` are cleared, and fields
    /// with a value replace the current one. Returns the freshly-written
    /// record.
    #[wasm_bindgen(js_name = updateAccountConfig)]
    pub async fn update_account_config(&self, updates_js: JsValue) -> Result<JsValue, JsError> {
        let updates: opake_core::records::AccountConfigUpdates =
            serde_wasm_bindgen::from_value(updates_js).map_err(|e| JsError::new(&e.to_string()))?;
        let mut opake = self.opake().await?;
        let record = opake
            .update_account_config(updates)
            .await
            .map_err(wasm_err)?;
        to_js(&record)
    }

    /// Publish the caller's public key record on the PDS.
    #[wasm_bindgen(js_name = publishPublicKey)]
    pub async fn publish_public_key(&self) -> Result<String, JsError> {
        let mut opake = self.opake().await?;
        opake.publish_public_key().await.map_err(wasm_err)
    }

    /// Discover workspaces the user is a member of (across all PDSes).
    #[wasm_bindgen(js_name = discoverMemberWorkspaces)]
    pub async fn discover_member_workspaces(&self) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let workspaces = opake.discover_member_workspaces().await.map_err(wasm_err)?;
        to_js(&workspaces)
    }

    /// Request a short-lived SSE token from the Indexer.
    #[wasm_bindgen(js_name = requestSseToken)]
    pub async fn request_sse_token(&self) -> Result<String, JsError> {
        let mut opake = self.opake().await?;
        opake.request_sse_token().await.map_err(wasm_err)
    }

    /// Fetch all incoming grants from the Indexer.
    ///
    /// Side effect: bootstraps the shared `InboxKeeper` with the result.
    /// Any `watchInbox` callers (current or future) receive a fresh
    /// snapshot with `loaded = true` as part of this call. Once
    /// bootstrapped, incremental SSE `grant:upsert` / `grant:delete`
    /// events keep the keeper in sync without further `listInbox`
    /// round-trips.
    #[wasm_bindgen(js_name = listInbox)]
    pub async fn list_inbox(&self) -> Result<JsValue, JsError> {
        // Gate-protected bootstrap (see `list_workspaces`). Returns the raw
        // grant envelopes for the JS wire format.
        let grants = crate::sse_wasm::bootstrap_inbox_keeper(
            &self.inner,
            &self.inbox_keeper,
            &self.inbox_gate,
            &self.sse_generation,
        )
        .await?;

        to_js(&grants)
    }

    /// List pending pair requests on this account.
    #[wasm_bindgen(js_name = listPairRequests)]
    pub async fn list_pair_requests(&self) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let entries = opake.list_pair_requests().await.map_err(wasm_err)?;
        to_js(&entries)
    }

    /// Delete all expired pair requests and orphaned responses (daemon use).
    #[wasm_bindgen(js_name = cleanupExpiredPairRequests)]
    pub async fn cleanup_expired_pair_requests(&self) -> Result<usize, JsError> {
        let mut opake = self.opake().await?;
        let ttl = opake_core::pairing::DEFAULT_PAIR_REQUEST_TTL_SECONDS;
        let result = opake
            .cleanup_expired_pair_requests(ttl)
            .await
            .map_err(wasm_err)?;
        Ok(result.requests_deleted + result.responses_deleted)
    }

    /// Delete stale grants whose recipients have no valid public key (daemon use).
    #[wasm_bindgen(js_name = healStaleGrants)]
    pub async fn heal_stale_grants(&self) -> Result<usize, JsError> {
        let mut opake = self.opake().await?;
        let result = opake.heal_stale_grants().await.map_err(wasm_err)?;
        Ok(result.grants_deleted)
    }

    /// Resolve another user's identity (DID, handle, public key).
    #[wasm_bindgen(js_name = resolveIdentity)]
    pub async fn resolve_identity(&self, handle_or_did: &str) -> Result<JsValue, JsError> {
        let opake = self.opake().await?;
        let resolved = opake
            .resolve_identity(handle_or_did)
            .await
            .map_err(wasm_err)?;

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct R {
            did: String,
            handle: Option<String>,
            pds_url: String,
            #[serde(with = "crate::wasm_util::serde_bytes")]
            x25519_public_key: Vec<u8>,
            x25519_algo: String,
            #[serde(with = "crate::wasm_util::serde_bytes")]
            ml_kem_public_key: Vec<u8>,
            ml_kem_algo: String,
        }

        to_js(&R {
            did: resolved.did,
            handle: resolved.handle,
            pds_url: resolved.pds_url,
            x25519_public_key: resolved.x25519_public_key.to_vec(),
            x25519_algo: resolved.x25519_algo,
            ml_kem_public_key: resolved.ml_kem_public_key.to_vec(),
            ml_kem_algo: resolved.ml_kem_algo,
        })
    }

    /// Resolve grant metadata without downloading the blob.
    #[wasm_bindgen(js_name = resolveGrantMetadata)]
    pub async fn resolve_grant_metadata(&self, grant_uri: &str) -> Result<JsValue, JsError> {
        let opake = self.opake().await?;
        let (name, metadata, created_at, modified_at) = opake
            .resolve_grant_metadata(grant_uri)
            .await
            .map_err(wasm_err)?;

        let resolved = opake_core::manager::ResolvedDocumentMetadata::from_parts(
            metadata,
            created_at,
            modified_at,
        );

        to_js(&crate::bindings::ResolvedGrantMetadataDto {
            name,
            metadata: crate::bindings::DocumentMetadataDto::from(&resolved),
        })
    }

    /// Proactively refresh the OAuth token if it's close to expiry.
    ///
    /// Calls `refresh_token` directly — no side-effect hacks. The refreshed
    /// session is persisted to storage.
    #[wasm_bindgen(js_name = proactiveRefresh)]
    pub async fn proactive_refresh(&self) -> Result<(), JsError> {
        use opake_core::client::session_refresh::{
            proactive_refresh, RefreshOutcome, DEFAULT_REFRESH_THRESHOLD_SECONDS,
        };

        let now = opake_core::client::time::unix_now();

        // Extract session + PDS URL while holding the lock, then drop it
        // so other WASM operations aren't blocked during the network call.
        let (session, pds_url) = {
            let mut opake = self.opake().await?;
            let needs_it = opake
                .session()
                .map(|s| s.needs_refresh(DEFAULT_REFRESH_THRESHOLD_SECONDS, now))
                .unwrap_or(false);
            if !needs_it {
                return Ok(());
            }
            let session = opake
                .session()
                .cloned()
                .ok_or_else(|| JsError::new("no session"))?;
            let pds_url = opake.client_mut().base_url().to_string();
            (session, pds_url)
        }; // guard dropped — Mutex free during network I/O

        let transport = opake_core::client::WasmTransport::new();
        let outcome = proactive_refresh(
            &transport,
            &session,
            &pds_url,
            DEFAULT_REFRESH_THRESHOLD_SECONDS,
            now,
            &mut opake_core::crypto::OsRng,
        )
        .await;

        match outcome {
            RefreshOutcome::Refreshed(new_session) => {
                // Re-acquire to persist the refreshed session
                let mut opake = self.opake().await?;
                opake
                    .persist_refreshed_session(&new_session)
                    .await
                    .map_err(wasm_err)?;
                Ok(())
            }
            RefreshOutcome::NotNeeded => Ok(()),
            RefreshOutcome::Failed(e) => Err(wasm_err(e)),
        }
    }

    /// Return the DID this Opake was constructed for.
    ///
    /// The DID is invariant for the lifetime of an OpakeContext — it's set
    /// once at `create()` time (either passed explicitly or resolved from
    /// the storage's default account) and never changes. Sync + cheap:
    /// the SDK calls this once during `Opake.init()` to populate a
    /// `readonly did: string` property, so JS consumers never have to
    /// round-trip into WASM to answer "who am I signed in as."
    ///
    /// Errors only on mutex contention via `try_lock`, which can't happen
    /// during the init window when the SDK calls it (no other code holds
    /// the lock before `Opake.init()` returns).
    #[wasm_bindgen(js_name = getDid)]
    pub fn get_did(&self) -> Result<String, JsError> {
        let guard = self
            .inner
            .try_lock()
            .ok_or_else(|| JsError::new("Opake is busy"))?;
        Ok(guard.did().to_string())
    }

    /// Get the token expiry timestamp without exposing the full session.
    ///
    /// Returns the Unix timestamp (seconds) when the access token expires,
    /// or -1 if unknown/not applicable (legacy sessions).
    /// This avoids serializing tokens/keys to JS for a simple expiry check.
    ///
    /// Uses try_lock — returns -1 if the Mutex is held. The SDK interprets
    /// -1 as "unknown expiry" and skips proactive refresh, which is correct —
    /// the in-flight operation holding the lock will complete first.
    #[wasm_bindgen(js_name = tokenExpiresAt)]
    pub fn token_expires_at(&self) -> f64 {
        let Some(guard) = self.inner.try_lock() else {
            return -1.0;
        };
        guard
            .session()
            .and_then(|s| s.expires_at())
            .map(|t| t as f64)
            .unwrap_or(-1.0)
    }

    /// Acquire the Opake mutex.
    ///
    /// Result-returning for API continuity with the callsites that predate
    /// the Option removal; the lock acquisition itself cannot fail in
    /// single-threaded WASM.
    async fn opake(&self) -> Result<OpakeGuard<'_>, JsError> {
        Ok(self.inner.lock().await)
    }
}

/// Shorthand for "locked guard onto the shared WasmOpake."
pub(crate) type OpakeGuard<'a> = futures_util::lock::MutexGuard<'a, WasmOpake>;

// FileManager is in file_manager_wasm.rs
