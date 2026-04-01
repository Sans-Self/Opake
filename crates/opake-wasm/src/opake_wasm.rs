// Stateful WASM exports for Opake domain types.
//
// JS callers construct an OpakeContext, then either:
// - Call workspace management methods directly (createWorkspace, listWorkspaces, etc.)
// - Call .cabinet() or .workspace() to get a FileManager for file operations
//
// FileManager shares ownership of the Opake via Rc<RefCell<>>. This lets
// the OpakeContext survive after creating a FileManager — .cabinet() and
// .workspace() are non-consuming. WASM is single-threaded, so RefCell is safe.

use std::cell::RefCell;
use std::rc::Rc;

use opake_core::manager::FileContext;
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
    inner: Rc<RefCell<Option<WasmOpake>>>,
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
        Ok(Self {
            inner: Rc::new(RefCell::new(Some(opake))),
        })
    }

    /// Create a cabinet FileManager. Non-consuming — the OpakeContext
    /// remains usable after the FileManager is freed.
    pub fn cabinet(&self) -> Result<WasmFileManagerHandle, JsError> {
        let borrow = self.inner.borrow();
        let opake = borrow
            .as_ref()
            .ok_or_else(|| JsError::new("Opake context already consumed"))?;
        let context = cabinet_context(opake)?;
        drop(borrow);

        Ok(WasmFileManagerHandle {
            opake: Rc::clone(&self.inner),
            context: Some(context),
        })
    }

    /// Create a workspace FileManager. Non-consuming — the OpakeContext
    /// remains usable after the FileManager is freed.
    pub fn workspace(
        &self,
        keyring_uri: &str,
        owner_did: &str,
        key: &[u8],
        rotation: u64,
    ) -> Result<WasmFileManagerHandle, JsError> {
        // Validate context is alive
        let borrow = self.inner.borrow();
        if borrow.is_none() {
            return Err(JsError::new("Opake context already consumed"));
        }
        drop(borrow);

        let context = workspace_context(keyring_uri, owner_did, key, rotation)?;
        Ok(WasmFileManagerHandle {
            opake: Rc::clone(&self.inner),
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
        let mut opake = self.opake()?;
        let ws = opake
            .resolve_workspace_by_uri(keyring_uri)
            .await
            .map_err(wasm_err)?;
        let context = opake_core::manager::FileContext::Workspace(ws);
        drop(opake);

        Ok(WasmFileManagerHandle {
            opake: Rc::clone(&self.inner),
            context: Some(context),
        })
    }

    // -- Workspace management (does NOT consume the context) --

    #[wasm_bindgen(js_name = createWorkspace)]
    pub async fn create_workspace(
        &mut self,
        name: &str,
        description: Option<String>,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
        let (keyring_uri, key) = opake
            .create_workspace(name, description.as_deref())
            .await
            .map_err(wasm_err)?;

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
    #[wasm_bindgen(js_name = listWorkspaces)]
    pub async fn list_workspaces(
        &mut self,
        default_appview_url: Option<String>,
    ) -> Result<JsValue, JsError> {
        use opake_core::crypto::{self, KeyringMetadata};
        use opake_core::records::{EncryptedMetadata, KeyringMember};

        let mut opake = self.opake()?;
        let identity = opake.require_identity().map_err(wasm_err)?;
        let private_key = identity.private_key_bytes().map_err(wasm_err)?;
        let did = opake.did().to_string();

        let keyrings = opake
            .discover_member_keyrings(default_appview_url.as_deref())
            .await
            .map_err(wasm_err)?;

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct WorkspaceEntry {
            uri: String,
            owner_did: String,
            rotation: u64,
            member_count: usize,
            created_at: Option<String>,
            name: Option<String>,
            description: Option<String>,
            icon: Option<String>,
            members: Vec<serde_json::Value>,
        }

        let mut entries = Vec::new();
        for kr in keyrings {
            let members_parsed: Vec<KeyringMember> = kr
                .members
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();

            let (name, description, icon) =
                match WasmOpake::unwrap_workspace_key(&members_parsed, &did, &private_key) {
                    Ok(group_key) => {
                        let meta_result = kr.encrypted_metadata.as_ref().and_then(|em| {
                            let em: EncryptedMetadata = serde_json::from_value(em.clone()).ok()?;
                            let meta: KeyringMetadata =
                                crypto::decrypt_metadata(&group_key, &em).ok()?;
                            Some(meta)
                        });
                        match meta_result {
                            Some(meta) => (Some(meta.name), meta.description, meta.icon),
                            None => (None, None, None),
                        }
                    }
                    Err(_) => (None, None, None),
                };

            entries.push(WorkspaceEntry {
                uri: kr.uri,
                owner_did: kr.owner_did,
                rotation: kr.rotation,
                member_count: kr.members.len(),
                created_at: kr.created_at,
                name,
                description,
                icon,
                members: kr.members,
            });
        }

        to_js(&serde_json::json!({ "keyrings": entries }))
    }

    #[wasm_bindgen(js_name = addWorkspaceMember)]
    pub async fn add_workspace_member(
        &mut self,
        keyring_uri: &str,
        key: &[u8],
        member_did: &str,
        member_public_key: &[u8],
        role: &str,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
        let gk = crate::content_key_from_slice(key)?;
        let pubkey = pub_key_from_slice(member_public_key)?;
        let role = parse_role(role)?;
        let outcome = opake
            .add_workspace_member(keyring_uri, &gk, member_did, &pubkey, role)
            .await
            .map_err(wasm_err)?;
        to_js(&MutationResultDto {
            uri: None,
            proposed: outcome.is_proposed(),
        })
    }

    #[wasm_bindgen(js_name = leaveWorkspace)]
    pub async fn leave_workspace(&mut self, keyring_uri: &str) -> Result<String, JsError> {
        let mut opake = self.opake()?;
        opake.leave_workspace(keyring_uri).await.map_err(wasm_err)
    }

    /// Remove a member from a workspace.
    /// Owner: rotates key, returns `{ key, rotation, proposed: false }`.
    /// Non-owner: proposes, returns `{ proposed: true }`.
    #[wasm_bindgen(js_name = removeWorkspaceMember)]
    pub async fn remove_workspace_member(
        &mut self,
        keyring_uri: &str,
        key: &[u8],
        member_did: &str,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
        let gk = crate::content_key_from_slice(key)?;
        let (key_result, outcome) = opake
            .remove_workspace_member(keyring_uri, &gk, member_did)
            .await
            .map_err(wasm_err)?;

        #[derive(Serialize)]
        struct R {
            #[serde(
                with = "crate::wasm_util::serde_bytes",
                skip_serializing_if = "Vec::is_empty"
            )]
            key: Vec<u8>,
            #[serde(skip_serializing_if = "Option::is_none")]
            rotation: Option<u64>,
            proposed: bool,
        }
        let (key_bytes, rotation) = match key_result {
            Some((k, r)) => (k.0.to_vec(), Some(r)),
            None => (Vec::new(), None),
        };
        serde_wasm_bindgen::to_value(&R {
            key: key_bytes,
            rotation,
            proposed: outcome.is_proposed(),
        })
        .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Update workspace metadata (name, description).
    /// Owner: applies directly. Non-owner: creates a keyringUpdate proposal.
    #[wasm_bindgen(js_name = updateWorkspaceMetadata)]
    pub async fn update_workspace_metadata(
        &mut self,
        keyring_uri: &str,
        key: &[u8],
        name: Option<String>,
        description: Option<String>,
        icon: Option<String>,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
        let gk = crate::content_key_from_slice(key)?;
        let outcome = opake
            .update_workspace_metadata(
                keyring_uri,
                &gk,
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
        &mut self,
        keyring_uri: &str,
        member_did: &str,
        role: &str,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
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
        &mut self,
        keyring_uri: &str,
        role: &str,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
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
    pub async fn list_invitations(&mut self) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
        let invitations = opake.list_invitations().await.map_err(wasm_err)?;

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
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
    pub async fn revoke_invitation(&mut self, invitation_uri: &str) -> Result<(), JsError> {
        let mut opake = self.opake()?;
        opake
            .revoke_invitation(invitation_uri)
            .await
            .map_err(wasm_err)
    }

    /// Accept an invitation by writing an acceptance record. Returns the acceptance URI.
    #[wasm_bindgen(js_name = acceptInvitation)]
    pub async fn accept_invitation(&mut self, invitation_uri: &str) -> Result<String, JsError> {
        let mut opake = self.opake()?;
        opake
            .accept_invitation(invitation_uri)
            .await
            .map_err(wasm_err)
    }

    #[wasm_bindgen(js_name = downloadFromGrant)]
    pub async fn download_from_grant(&mut self, grant_uri: &str) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
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
    pub async fn create_pair_request(&mut self) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
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
        &mut self,
        request_uri: &str,
        ephemeral_public_key: &[u8],
    ) -> Result<(), JsError> {
        let pubkey = pub_key_from_slice(ephemeral_public_key)?;
        let mut opake = self.opake()?;
        opake
            .approve_pair_request(request_uri, &pubkey)
            .await
            .map_err(wasm_err)
    }

    /// Receive a pair response (new device side). Returns the derived Identity.
    #[wasm_bindgen(js_name = receivePairResponse)]
    pub async fn receive_pair_response(
        &mut self,
        response_js: JsValue,
        ephemeral_private_key: &[u8],
    ) -> Result<JsValue, JsError> {
        let response: opake_core::records::PairResponse =
            serde_wasm_bindgen::from_value(response_js)
                .map_err(|e| JsError::new(&e.to_string()))?;
        let privkey: opake_core::crypto::X25519PrivateKey = ephemeral_private_key
            .try_into()
            .map_err(|_| JsError::new("ephemeral private key must be 32 bytes"))?;
        let mut opake = self.opake()?;
        let identity = opake
            .receive_pair_response(&response, &privkey)
            .await
            .map_err(wasm_err)?;
        to_js(&identity)
    }

    /// Sync all owned workspaces and apply pending directory proposals.
    #[wasm_bindgen(js_name = syncOwnedWorkspaces)]
    pub async fn sync_owned_workspaces(&mut self) -> Result<usize, JsError> {
        let mut opake = self.opake()?;
        opake.sync_owned_workspaces().await.map_err(wasm_err)
    }

    /// Sync all workspaces with per-workspace result visibility.
    #[wasm_bindgen(js_name = syncOwnedWorkspacesDetailed)]
    pub async fn sync_owned_workspaces_detailed(&mut self) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
        let results = opake
            .sync_owned_workspaces_detailed()
            .await
            .map_err(wasm_err)?;
        to_js(&results)
    }

    /// Retry all pending shares (resolve recipients, create grants).
    #[wasm_bindgen(js_name = retryPendingSharesViaOpake)]
    pub async fn retry_pending_shares_via_opake(&mut self) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
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

    /// Fetch the account config record, if it exists.
    #[wasm_bindgen(js_name = getAccountConfig)]
    pub async fn get_account_config(&mut self) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
        let config = opake.get_account_config().await.map_err(wasm_err)?;
        to_js(&config)
    }

    /// Write the account config record (upsert).
    #[wasm_bindgen(js_name = setAccountConfig)]
    pub async fn set_account_config(&mut self, config_js: JsValue) -> Result<String, JsError> {
        let config: opake_core::records::AccountConfigRecord =
            serde_wasm_bindgen::from_value(config_js).map_err(|e| JsError::new(&e.to_string()))?;
        let mut opake = self.opake()?;
        opake.set_account_config(&config).await.map_err(wasm_err)
    }

    /// Publish the caller's public key record on the PDS.
    #[wasm_bindgen(js_name = publishPublicKey)]
    pub async fn publish_public_key(&mut self) -> Result<String, JsError> {
        let mut opake = self.opake()?;
        opake.publish_public_key().await.map_err(wasm_err)
    }

    /// Fetch workspace documents from the AppView.
    #[wasm_bindgen(js_name = listWorkspaceDocuments)]
    pub async fn list_workspace_documents(
        &mut self,
        keyring_uri: &str,
        default_appview_url: Option<String>,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
        let docs = opake
            .list_workspace_documents(keyring_uri, default_appview_url.as_deref())
            .await
            .map_err(wasm_err)?;
        to_js(&docs)
    }

    /// Discover keyrings the user is a member of (across all PDSes).
    #[wasm_bindgen(js_name = discoverMemberKeyrings)]
    pub async fn discover_member_keyrings(
        &mut self,
        default_appview_url: Option<String>,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
        let keyrings = opake
            .discover_member_keyrings(default_appview_url.as_deref())
            .await
            .map_err(wasm_err)?;
        to_js(&keyrings)
    }

    /// Fetch all incoming grants from the AppView.
    #[wasm_bindgen(js_name = listInbox)]
    pub async fn list_inbox(&mut self, appview_url: Option<String>) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
        let grants = opake
            .list_inbox(appview_url.as_deref())
            .await
            .map_err(wasm_err)?;
        to_js(&grants)
    }

    /// Resolve a workspace from a foreign PDS by keyring URI.
    #[wasm_bindgen(js_name = resolveForeignWorkspace)]
    pub async fn resolve_foreign_workspace(
        &mut self,
        keyring_uri: &str,
    ) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
        let workspace = opake
            .resolve_foreign_workspace(keyring_uri)
            .await
            .map_err(wasm_err)?;

        #[derive(Serialize)]
        struct R {
            uri: String,
            name: String,
            owner_did: String,
            rotation: u64,
            #[serde(with = "crate::wasm_util::serde_bytes")]
            group_key: Vec<u8>,
        }

        let result = R {
            uri: workspace.uri.clone(),
            name: workspace.name.clone(),
            owner_did: workspace.owner_did.clone(),
            rotation: workspace.rotation,
            group_key: workspace.key.0.to_vec(),
        };
        to_js(&result)
    }

    /// List pending pair requests on this account.
    #[wasm_bindgen(js_name = listPairRequests)]
    pub async fn list_pair_requests(&mut self) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
        let entries = opake.list_pair_requests().await.map_err(wasm_err)?;
        to_js(&entries)
    }

    /// List pair responses on this account.
    #[wasm_bindgen(js_name = listPairResponses)]
    pub async fn list_pair_responses(&mut self) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
        let entries = opake.list_pair_responses().await.map_err(wasm_err)?;
        to_js(&entries)
    }

    /// Clean up pair request + response records after successful pairing.
    #[wasm_bindgen(js_name = cleanupPairRecords)]
    pub async fn cleanup_pair_records(
        &mut self,
        request_rkey: &str,
        response_rkey: &str,
    ) -> Result<(), JsError> {
        let mut opake = self.opake()?;
        opake
            .cleanup_pair_records(request_rkey, response_rkey)
            .await
            .map_err(wasm_err)
    }

    /// Delete all expired pair requests and orphaned responses (daemon use).
    #[wasm_bindgen(js_name = cleanupExpiredPairRequests)]
    pub async fn cleanup_expired_pair_requests(&mut self) -> Result<usize, JsError> {
        let mut opake = self.opake()?;
        let ttl = opake_core::pairing::DEFAULT_PAIR_REQUEST_TTL_SECONDS;
        let result = opake
            .cleanup_expired_pair_requests(ttl)
            .await
            .map_err(wasm_err)?;
        Ok(result.requests_deleted + result.responses_deleted)
    }

    /// Delete stale grants whose recipients have no valid public key (daemon use).
    #[wasm_bindgen(js_name = healStaleGrants)]
    pub async fn heal_stale_grants(&mut self) -> Result<usize, JsError> {
        let mut opake = self.opake()?;
        let result = opake.heal_stale_grants().await.map_err(wasm_err)?;
        Ok(result.grants_deleted)
    }

    /// Resolve another user's identity (DID, handle, public key).
    #[wasm_bindgen(js_name = resolveIdentity)]
    pub async fn resolve_identity(&mut self, handle_or_did: &str) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
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
    pub async fn resolve_grant_metadata(&mut self, grant_uri: &str) -> Result<JsValue, JsError> {
        let mut opake = self.opake()?;
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

    /// Unwrap a workspace group key using the caller's identity (no raw key params).
    ///
    /// Reads the private key from the Opake's identity — the key never
    /// crosses the WASM/JS boundary.
    #[wasm_bindgen(js_name = unwrapGroupKey)]
    pub fn unwrap_group_key(&self, members_js: JsValue) -> Result<Vec<u8>, JsError> {
        let borrow = self.inner.borrow();
        let opake = borrow
            .as_ref()
            .ok_or_else(|| JsError::new("Opake context already consumed"))?;
        let identity = opake.require_identity().map_err(wasm_err)?;
        let private_key = identity.private_key_bytes().map_err(wasm_err)?;
        let members: Vec<opake_core::records::KeyringMember> =
            serde_wasm_bindgen::from_value(members_js).map_err(|e| JsError::new(&e.to_string()))?;
        let key = WasmOpake::unwrap_workspace_key(&members, opake.did(), &private_key)
            .map_err(wasm_err)?;
        Ok(key.0.to_vec())
    }

    /// Unwrap a workspace key from keyring members (synchronous, no network).
    /// REMOVE: prefer unwrapGroupKey which reads identity internally.
    #[wasm_bindgen(js_name = unwrapWorkspaceKey)]
    pub fn unwrap_workspace_key(
        members_js: JsValue,
        did: &str,
        private_key: &[u8],
    ) -> Result<Vec<u8>, JsError> {
        let members: Vec<opake_core::records::KeyringMember> =
            serde_wasm_bindgen::from_value(members_js).map_err(|e| JsError::new(&e.to_string()))?;
        let privkey: opake_core::crypto::X25519PrivateKey = private_key
            .try_into()
            .map_err(|_| JsError::new("private key must be 32 bytes"))?;
        let key = WasmOpake::unwrap_workspace_key(&members, did, &privkey).map_err(wasm_err)?;
        Ok(key.0.to_vec())
    }

    /// Get the (potentially refreshed) session.
    pub fn session(&self) -> Result<JsValue, JsError> {
        let borrow = self.inner.borrow();
        let opake = borrow
            .as_ref()
            .ok_or_else(|| JsError::new("already consumed"))?;
        let session = opake.session().ok_or_else(|| JsError::new("no session"))?;
        serde_wasm_bindgen::to_value(session).map_err(|e| JsError::new(&e.to_string()))
    }

    fn opake(&self) -> Result<std::cell::RefMut<'_, WasmOpake>, JsError> {
        let borrow = self.inner.borrow_mut();
        std::cell::RefMut::filter_map(borrow, |opt| opt.as_mut())
            .map_err(|_| JsError::new("Opake context already consumed"))
    }
}

// FileManager is in file_manager_wasm.rs
