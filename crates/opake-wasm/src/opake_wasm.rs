// Stateful WASM exports for Opake domain types.
//
// JS callers construct an OpakeContext, then either:
// - Call workspace management methods directly (createWorkspace, listWorkspaces, etc.)
// - Call .cabinet() or .workspace() to get a FileManager for file operations
//
// FileManager shares access via Rc<Mutex<>>. Both OpakeContext and
// FileManager hold Rc clones. If the inner Option is set to None,
// FileManager methods fail cleanly.
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
use opake_core::indexer::inbox_keeper::{self as ik, InboxKeeper};
use opake_core::indexer::tree_keeper::TreeKeeper;
use opake_core::indexer::workspace_keeper::WorkspaceKeeper;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::file_manager_wasm::WasmFileManagerHandle;
use crate::js_storage::JsStorageAdapter;
use crate::wasm_util::{
    cabinet_context, make_opake_from_storage, parse_role, pub_key_from_slice, to_js, wasm_err,
    workspace_context, DownloadResult, MutationResultDto, WasmOpake,
};

// ---------------------------------------------------------------------------
// OpakeContext
// ---------------------------------------------------------------------------

#[wasm_bindgen(js_name = OpakeContext)]
pub struct WasmOpakeHandle {
    pub(crate) inner: Rc<Mutex<Option<WasmOpake>>>,
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
    /// `true` while an SSE consumer task is alive. Doubles as both the
    /// idempotency gate on `startSseConsumer` and the cancellation
    /// signal read by the consumer loop — `stopSseConsumer` clears it,
    /// the loop checks it after each `next_event().await` and breaks
    /// on `false`. Single-threaded WASM — `Cell<bool>` is enough.
    pub(crate) sse_started: Rc<std::cell::Cell<bool>>,
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
            inner: Rc::new(Mutex::new(Some(opake))),
            tree_keeper: Rc::new(Mutex::new(TreeKeeper::new(did_owned))),
            workspace_keeper: Rc::new(Mutex::new(WorkspaceKeeper::new())),
            inbox_keeper: Rc::new(Mutex::new(InboxKeeper::new())),
            sse_started: Rc::new(std::cell::Cell::new(false)),
        })
    }

    /// Create a cabinet FileManager. Non-consuming — the OpakeContext
    /// remains usable after the FileManager is freed.
    pub async fn cabinet(&self) -> Result<WasmFileManagerHandle, JsError> {
        let guard = self.inner.lock().await;
        let opake = guard
            .as_ref()
            .ok_or_else(|| JsError::new("Opake context already consumed"))?;
        let context = cabinet_context(opake)?;
        drop(guard);

        Ok(WasmFileManagerHandle {
            opake: Rc::clone(&self.inner),
            tree_keeper: Rc::clone(&self.tree_keeper),
            context: Some(context),
        })
    }

    /// Create a workspace FileManager. Non-consuming — the OpakeContext
    /// remains usable after the FileManager is freed.
    pub async fn workspace(
        &self,
        keyring_uri: &str,
        owner_did: &str,
        key: &[u8],
        rotation: u64,
    ) -> Result<WasmFileManagerHandle, JsError> {
        // Validate context is alive
        let guard = self.inner.lock().await;
        if guard.is_none() {
            return Err(JsError::new("Opake context already consumed"));
        }
        drop(guard);

        let context = workspace_context(keyring_uri, owner_did, key, rotation)?;
        Ok(WasmFileManagerHandle {
            opake: Rc::clone(&self.inner),
            tree_keeper: Rc::clone(&self.tree_keeper),
            context: Some(context),
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
            context: Some(context),
        })
    }

    // -- Workspace management (does NOT consume the context) --

    /// List members of a workspace. Fetches the keyring record and returns
    /// the member list with DIDs and roles.
    #[wasm_bindgen(js_name = listWorkspaceMembers)]
    pub async fn list_workspace_members(&self, keyring_uri: &str) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let at_uri = opake_core::atproto::parse_at_uri(keyring_uri).map_err(wasm_err)?;
        let entry = opake
            .client_mut()
            .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
            .await
            .map_err(wasm_err)?;
        let keyring: opake_core::records::Keyring =
            serde_json::from_value(entry.value).map_err(|e| JsError::new(&e.to_string()))?;
        to_js(&keyring.members)
    }

    #[wasm_bindgen(js_name = createWorkspace)]
    pub async fn create_workspace(
        &self,
        name: &str,
        description: Option<String>,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let (keyring_uri, key) = opake
            .create_workspace(name, description.as_deref())
            .await
            .map_err(wasm_err)?;

        // Optimistic insert — the sidebar shows the new workspace immediately
        // rather than waiting 1–4s for the SSE echo to arrive. The echo
        // produces an equal entry and the keeper's dedup short-circuits.
        let optimistic = opake_core::indexer::workspace_keeper::WorkspaceEntry {
            uri: keyring_uri.clone(),
            owner_did: opake.did().to_string(),
            rotation: 1,
            member_count: 1,
            created_at: Some(opake.now()),
            name: Some(name.to_string()),
            description: description.clone(),
            icon: None,
            my_role: Some("manager".to_string()),
        };
        drop(opake);

        {
            let mut keeper = self.workspace_keeper.lock().await;
            keeper.upsert(optimistic);
        }

        #[derive(Serialize)]
        struct R {
            keyring_uri: String,
            #[serde(with = "crate::wasm_util::serde_bytes")]
            key: Vec<u8>,
        }

        serde_wasm_bindgen::to_value(&R {
            keyring_uri,
            key: key.0.to_vec(),
        })
        .map_err(|e| JsError::new(&e.to_string()))
    }

    /// List all keyrings the user is a member of, with decrypted metadata.
    ///
    /// Side effect: bootstraps the shared `WorkspaceKeeper` with the
    /// result. Any `watchWorkspaces` callers (current or future) receive
    /// a fresh snapshot with `loaded = true` as part of this call. Once
    /// bootstrapped, incremental SSE events keep the keeper in sync
    /// without further `listWorkspaces` round-trips.
    #[wasm_bindgen(js_name = listWorkspaces)]
    pub async fn list_workspaces(
        &self,
        default_indexer_url: Option<String>,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let identity = opake.require_identity().map_err(wasm_err)?;
        let private_key = identity.private_key_bytes().map_err(wasm_err)?;
        let did = opake.did().to_string();

        let keyrings = opake
            .discover_member_keyrings(default_indexer_url.as_deref())
            .await
            .map_err(wasm_err)?;
        drop(opake);

        // Build entries via the shared `workspace_keeper::try_build_entry`
        // helper so this path and the SSE event path produce identical
        // WorkspaceEntry values — any shape divergence between them
        // would cause spurious watcher re-fires after SSE echoes.
        let entries: Vec<opake_core::indexer::workspace_keeper::WorkspaceEntry> = keyrings
            .iter()
            .filter_map(|kr| {
                opake_core::indexer::workspace_keeper::try_build_entry_from_indexer_keyring(
                    kr,
                    &did,
                    &private_key,
                )
            })
            .collect();

        // Bootstrap the keeper before returning — subscribers see the
        // loaded state before any caller that awaits this method's
        // return value resumes.
        {
            let mut keeper = self.workspace_keeper.lock().await;
            keeper.bootstrap(entries.clone());
        }

        // Wire-format for backward compatibility with the existing Zod
        // schema — `{ keyrings: [...] }` with snake_case fields.
        to_js(&serde_json::json!({ "keyrings": entries }))
    }

    /// Add a member to a workspace. Resolves the keyring + group key
    /// internally so the key never crosses the WASM/JS boundary.
    #[wasm_bindgen(js_name = addWorkspaceMember)]
    pub async fn add_workspace_member(
        &self,
        keyring_uri: &str,
        member_did: &str,
        member_public_key: &[u8],
        role: &str,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let ws = opake
            .resolve_workspace_by_uri(keyring_uri)
            .await
            .map_err(wasm_err)?;
        let pubkey = pub_key_from_slice(member_public_key)?;
        let role = parse_role(role)?;
        let outcome = opake
            .add_workspace_member(keyring_uri, &ws.key, member_did, &pubkey, role)
            .await
            .map_err(wasm_err)?;
        to_js(&MutationResultDto {
            uri: None,
            proposed: outcome.is_proposed(),
        })
    }

    #[wasm_bindgen(js_name = leaveWorkspace)]
    pub async fn leave_workspace(&self, keyring_uri: &str) -> Result<String, JsError> {
        let mut opake = self.opake().await?;
        opake.leave_workspace(keyring_uri).await.map_err(wasm_err)
    }

    /// Remove a member from a workspace. Resolves the keyring + group key
    /// internally so the key never crosses the WASM/JS boundary.
    ///
    /// Owner: rotates the group key in-place and re-wraps to remaining
    /// members — the new key stays inside WASM. Non-owner: writes a
    /// keyringUpdate proposal. In both cases the returned DTO carries
    /// only `{ proposed, rotation }`; the rotated key bytes are dropped
    /// (they'd be a boundary violation) and the next operation re-resolves
    /// via `resolve_workspace_by_uri`.
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
        let (key_result, outcome) = opake
            .remove_workspace_member(keyring_uri, &ws.key, member_did)
            .await
            .map_err(wasm_err)?;

        #[derive(Serialize)]
        struct R {
            #[serde(skip_serializing_if = "Option::is_none")]
            rotation: Option<u64>,
            proposed: bool,
        }
        serde_wasm_bindgen::to_value(&R {
            rotation: key_result.map(|(_, r)| r),
            proposed: outcome.is_proposed(),
        })
        .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Update workspace metadata (name, description, icon). Resolves the
    /// keyring + group key internally so the key never crosses the WASM/JS
    /// boundary.
    ///
    /// Owner: applies directly. Non-owner: creates a keyringUpdate proposal.
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
        let outcome = opake
            .update_workspace_metadata(
                keyring_uri,
                &ws.key,
                name.as_deref(),
                description.as_deref(),
                icon.as_deref(),
            )
            .await
            .map_err(wasm_err)?;
        to_js(&MutationResultDto {
            uri: None,
            proposed: outcome.is_proposed(),
        })
    }

    /// Update a workspace member's role.
    #[wasm_bindgen(js_name = updateMemberRole)]
    pub async fn update_member_role(
        &self,
        keyring_uri: &str,
        member_did: &str,
        role: &str,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let role = parse_role(role)?;
        let outcome = opake
            .update_member_role(keyring_uri, member_did, role)
            .await
            .map_err(wasm_err)?;
        to_js(&MutationResultDto {
            uri: None,
            proposed: outcome.is_proposed(),
        })
    }

    /// Download and decrypt a file using a grant (cross-PDS, recipient side).
    // -- Invitations --

    /// Create a workspace invitation. Returns `{ uri, token }`.
    #[wasm_bindgen(js_name = createInvitation)]
    pub async fn create_invitation(
        &self,
        keyring_uri: &str,
        role: &str,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let (uri, token) = opake
            .create_invitation(keyring_uri, role)
            .await
            .map_err(wasm_err)?;

        #[derive(Serialize)]
        struct R {
            uri: String,
            token: String,
        }
        serde_wasm_bindgen::to_value(&R { uri, token }).map_err(|e| JsError::new(&e.to_string()))
    }

    /// List all invitations on the caller's PDS.
    #[wasm_bindgen(js_name = listInvitations)]
    pub async fn list_invitations(&self) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let invitations = opake.list_invitations().await.map_err(wasm_err)?;

        #[derive(Serialize)]
        struct Entry {
            uri: String,
            target: String,
            invitation_type: String,
            role: Option<String>,
            token: String,
            max_uses: Option<u32>,
            uses: u32,
            expires_at: Option<String>,
            created_at: String,
        }

        let entries: Vec<Entry> = invitations
            .into_iter()
            .map(|(uri, inv)| Entry {
                uri,
                target: inv.target,
                invitation_type: inv.invitation_type,
                role: inv.role,
                token: inv.token,
                max_uses: inv.max_uses,
                uses: inv.uses,
                expires_at: inv.expires_at,
                created_at: inv.created_at,
            })
            .collect();

        serde_wasm_bindgen::to_value(&entries).map_err(|e| JsError::new(&e.to_string()))
    }

    /// Revoke (delete) an invitation.
    #[wasm_bindgen(js_name = revokeInvitation)]
    pub async fn revoke_invitation(&self, invitation_uri: &str) -> Result<(), JsError> {
        let mut opake = self.opake().await?;
        opake
            .revoke_invitation(invitation_uri)
            .await
            .map_err(wasm_err)
    }

    /// Accept an invitation by writing an acceptance record. Returns the acceptance URI.
    #[wasm_bindgen(js_name = acceptInvitation)]
    pub async fn accept_invitation(&self, invitation_uri: &str) -> Result<String, JsError> {
        let mut opake = self.opake().await?;
        opake
            .accept_invitation(invitation_uri)
            .await
            .map_err(wasm_err)
    }

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

    /// Create a pair request (new device side). Returns { uri, rkey, ephemeralPublicKey, ephemeralPrivateKey }.
    #[wasm_bindgen(js_name = createPairRequest)]
    pub async fn create_pair_request(&self) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let (record_ref, keypair) = opake.create_pair_request().await.map_err(wasm_err)?;

        #[derive(Serialize)]
        struct R {
            uri: String,
            rkey: String,
            #[serde(with = "crate::wasm_util::serde_bytes")]
            ephemeral_public_key: Vec<u8>,
            #[serde(with = "crate::wasm_util::serde_bytes")]
            ephemeral_private_key: Vec<u8>,
        }

        let rkey = opake_core::atproto::parse_at_uri(&record_ref.uri)
            .map(|u| u.rkey)
            .unwrap_or_default();

        to_js(&R {
            uri: record_ref.uri,
            rkey,
            ephemeral_public_key: keypair.public_key.to_vec(),
            ephemeral_private_key: keypair.private_key.to_vec(),
        })
    }

    /// Approve a pair request (existing device side). Creates the pair response record.
    #[wasm_bindgen(js_name = approvePairRequest)]
    pub async fn approve_pair_request(
        &self,
        request_uri: &str,
        ephemeral_public_key: &[u8],
    ) -> Result<(), JsError> {
        let pubkey = pub_key_from_slice(ephemeral_public_key)?;
        let mut opake = self.opake().await?;
        opake
            .approve_pair_request(request_uri, &pubkey)
            .await
            .map_err(wasm_err)
    }

    /// Receive a pair response (new device side). Returns the derived Identity.
    #[wasm_bindgen(js_name = receivePairResponse)]
    pub async fn receive_pair_response(
        &self,
        response_js: JsValue,
        ephemeral_private_key: &[u8],
    ) -> Result<JsValue, JsError> {
        let response: opake_core::records::PairResponse =
            serde_wasm_bindgen::from_value(response_js)
                .map_err(|e| JsError::new(&e.to_string()))?;
        let privkey: opake_core::crypto::X25519PrivateKey = ephemeral_private_key
            .try_into()
            .map_err(|_| JsError::new("ephemeral private key must be 32 bytes"))?;
        let mut opake = self.opake().await?;
        let identity = opake
            .receive_pair_response(&response, &privkey)
            .await
            .map_err(wasm_err)?;
        to_js(&identity)
    }

    /// Sync a single workspace by keyring URI. Returns null if the URI is not
    /// in the member list, or the sync result otherwise.
    #[wasm_bindgen(js_name = syncWorkspaceByUri)]
    pub async fn sync_workspace_by_uri(&self, keyring_uri: &str) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let result = opake
            .sync_workspace_by_uri(keyring_uri)
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
    pub async fn set_indexer_url(&self, url: String) -> Result<(), JsError> {
        let mut guard = self.inner.lock().await;
        let opake = guard
            .as_mut()
            .ok_or_else(|| JsError::new("Opake context already consumed"))?;
        opake.set_indexer_url(url);
        Ok(())
    }

    /// Verify the session is usable by touching the account config record.
    ///
    /// Reads the config, stamps `modifiedAt`, and writes it back. Throws on
    /// auth failure — the SDK uses this during boot to detect dead sessions.
    #[wasm_bindgen(js_name = checkSession)]
    pub async fn check_session(&self) -> Result<(), JsError> {
        let mut opake = self.opake().await?;
        let config = opake.get_account_config().await.map_err(wasm_err)?;
        let mut record = config
            .unwrap_or_else(|| opake_core::records::AccountConfigRecord::new(&crate::now_iso()));
        record.modified_at = crate::now_iso();
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

    /// Fetch workspace documents from the Indexer.
    #[wasm_bindgen(js_name = listWorkspaceDocuments)]
    pub async fn list_workspace_documents(
        &self,
        keyring_uri: &str,
        default_indexer_url: Option<String>,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let docs = opake
            .list_workspace_documents(keyring_uri, default_indexer_url.as_deref())
            .await
            .map_err(wasm_err)?;
        to_js(&docs)
    }

    /// Discover keyrings the user is a member of (across all PDSes).
    #[wasm_bindgen(js_name = discoverMemberKeyrings)]
    pub async fn discover_member_keyrings(
        &self,
        default_indexer_url: Option<String>,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let keyrings = opake
            .discover_member_keyrings(default_indexer_url.as_deref())
            .await
            .map_err(wasm_err)?;
        to_js(&keyrings)
    }

    /// Request a short-lived SSE token from the Indexer.
    #[wasm_bindgen(js_name = requestSseToken)]
    pub async fn request_sse_token(&self, indexer_url: Option<String>) -> Result<String, JsError> {
        let mut opake = self.opake().await?;
        opake
            .request_sse_token(indexer_url.as_deref())
            .await
            .map_err(wasm_err)
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
    pub async fn list_inbox(&self, indexer_url: Option<String>) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let grants = opake
            .list_inbox(indexer_url.as_deref())
            .await
            .map_err(wasm_err)?;
        drop(opake);

        let entries: Vec<opake_core::indexer::inbox_keeper::InboxEntry> =
            grants.iter().map(ik::entry_from_indexer_grant).collect();

        {
            let mut keeper = self.inbox_keeper.lock().await;
            keeper.bootstrap(entries);
        }

        to_js(&grants)
    }

    /// List pending pair requests on this account.
    #[wasm_bindgen(js_name = listPairRequests)]
    pub async fn list_pair_requests(&self) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let entries = opake.list_pair_requests().await.map_err(wasm_err)?;
        to_js(&entries)
    }

    /// List pair responses on this account.
    #[wasm_bindgen(js_name = listPairResponses)]
    pub async fn list_pair_responses(&self) -> Result<JsValue, JsError> {
        let mut opake = self.opake().await?;
        let entries = opake.list_pair_responses().await.map_err(wasm_err)?;
        to_js(&entries)
    }

    /// Clean up pair request + response records after successful pairing.
    #[wasm_bindgen(js_name = cleanupPairRecords)]
    pub async fn cleanup_pair_records(
        &self,
        request_rkey: &str,
        response_rkey: &str,
    ) -> Result<(), JsError> {
        let mut opake = self.opake().await?;
        opake
            .cleanup_pair_records(request_rkey, response_rkey)
            .await
            .map_err(wasm_err)
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
        struct R {
            did: String,
            handle: Option<String>,
            pds_url: String,
            #[serde(with = "crate::wasm_util::serde_bytes")]
            public_key: Vec<u8>,
            algo: String,
        }

        to_js(&R {
            did: resolved.did,
            handle: resolved.handle,
            pds_url: resolved.pds_url,
            public_key: resolved.public_key.to_vec(),
            algo: resolved.algo,
        })
    }

    /// Resolve grant metadata without downloading the blob.
    #[wasm_bindgen(js_name = resolveGrantMetadata)]
    pub async fn resolve_grant_metadata(&self, grant_uri: &str) -> Result<JsValue, JsError> {
        let opake = self.opake().await?;
        let (name, metadata) = opake
            .resolve_grant_metadata(grant_uri)
            .await
            .map_err(wasm_err)?;

        #[derive(serde::Serialize)]
        struct R {
            name: String,
            metadata: opake_core::crypto::DocumentMetadata,
        }

        to_js(&R { name, metadata })
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

    /// Get the (potentially refreshed) session.
    pub fn session(&self) -> Result<JsValue, JsError> {
        let guard = self
            .inner
            .try_lock()
            .ok_or_else(|| JsError::new("Opake is busy — an operation is in progress"))?;
        let opake = guard
            .as_ref()
            .ok_or_else(|| JsError::new("already consumed"))?;
        let session = opake.session().ok_or_else(|| JsError::new("no session"))?;
        serde_wasm_bindgen::to_value(session).map_err(|e| JsError::new(&e.to_string()))
    }

    /// Get the authenticated DID without exposing the full session.
    ///
    /// Returns the DID string, or an error if the context is busy or has
    /// no session. Used for self-event filtering in the SSE consumer.
    #[wasm_bindgen(js_name = getDid)]
    pub fn get_did(&self) -> Result<String, JsError> {
        let guard = self
            .inner
            .try_lock()
            .ok_or_else(|| JsError::new("Opake is busy"))?;
        let opake = guard
            .as_ref()
            .ok_or_else(|| JsError::new("already consumed"))?;
        Ok(opake.did().to_string())
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
        let Some(opake) = guard.as_ref() else {
            return -1.0;
        };
        opake
            .session()
            .and_then(|s| s.expires_at())
            .map(|t| t as f64)
            .unwrap_or(-1.0)
    }

    async fn opake(&self) -> Result<OpakeGuard<'_>, JsError> {
        let guard = self.inner.lock().await;
        if guard.is_none() {
            return Err(JsError::new("Opake context already consumed"));
        }
        Ok(OpakeGuard(guard))
    }
}

/// Newtype over MutexGuard that derefs to WasmOpake (unwraps the Option).
/// The Option is checked in `opake()` — callers can use this like `&mut WasmOpake`.
pub(crate) struct OpakeGuard<'a>(pub(crate) futures_util::lock::MutexGuard<'a, Option<WasmOpake>>);

impl std::ops::Deref for OpakeGuard<'_> {
    type Target = WasmOpake;
    fn deref(&self) -> &WasmOpake {
        self.0.as_ref().unwrap()
    }
}

impl std::ops::DerefMut for OpakeGuard<'_> {
    fn deref_mut(&mut self) -> &mut WasmOpake {
        self.0.as_mut().unwrap()
    }
}

// FileManager is in file_manager_wasm.rs
