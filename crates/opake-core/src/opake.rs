// Opake: root context for all Opake operations.
//
// Bundles the authenticated PDS client, user identity, cryptographic
// randomness, platform time, and storage. Construct once per operation,
// then call `file_context()` + `file_manager()` to get a FileManager,
// or use workspace management methods directly.
//
// Platform differences are handled by the type parameters:
// - T: Transport (ReqwestTransport for CLI, WasmTransport for browser)
// - R: CryptoRng + RngCore (OsRng for both, ChaCha8Rng for tests)
// - S: Storage (FileStorage for CLI, NoopStorage for WASM until IndexedDb lands)
//
// Time is injected as a function pointer — CLI passes chrono, WASM passes
// js_sys::Date. No captures, no allocation.
//
// Session persistence is automatic: after any XRPC call that triggers a
// token refresh, Opake persists the new session through Storage.

use crate::atproto;
use crate::cabinet::Cabinet;
use crate::client::{Transport, XrpcClient};
use crate::crypto::{ContentKey, CryptoRng, DidMember, RngCore, X25519PublicKey};
use crate::error::Error;
use crate::keyrings::{self, AddMemberParams, CreateKeyringParams, KEYRING_COLLECTION};
use crate::manager::MutationOutcome;
use crate::manager::{FileContext, FileManager, WorkspaceAdmin};
use crate::records::{
    Invitation, InvitationAcceptance, KeyringUpdateRecord, Role, INVITATION_ACCEPTANCE_COLLECTION,
    INVITATION_COLLECTION, KEYRING_UPDATE_COLLECTION,
};
use crate::storage::{Identity, Storage};
use crate::workspace::Workspace;

pub struct Opake<T: Transport, R: CryptoRng + RngCore, S: Storage> {
    pub(crate) client: XrpcClient<T>,
    pub(crate) did: String,
    pub(crate) identity: Option<Identity>,
    pub(crate) rng: R,
    pub(crate) storage: S,
    pub(crate) now_fn: fn() -> String,
    pub(crate) now_micros_fn: fn() -> u64,
    /// Cached appview URL from account config (fetched once at construction).
    pub(crate) appview_url: Option<String>,
}

/// AppView URL baked into the binary at compile time.
///
/// Set via `OPAKE_APPVIEW_URL` env var at build time. Falls back to
/// the production URL if unset. Dev builds pick it up from `.envrc`.
pub const DEFAULT_APPVIEW_URL: &str = match option_env!("OPAKE_APPVIEW_URL") {
    Some(url) => url,
    None => "https://appview.opake.app",
};

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> Opake<T, R, S> {
    pub fn new(
        client: XrpcClient<T>,
        did: String,
        identity: Option<Identity>,
        rng: R,
        storage: S,
        now: fn() -> String,
        now_micros: fn() -> u64,
    ) -> Self {
        Self {
            client,
            did,
            identity,
            rng,
            storage,
            now_fn: now,
            now_micros_fn: now_micros,
            appview_url: None,
        }
    }

    // -- Factory --

    /// Build an Opake for a specific account, reading from Storage.
    ///
    /// Loads config (to resolve DID + PDS URL), session (authentication),
    /// and identity (encryption keys, if present). If the identity exists
    /// but lacks signing keys, migrates it automatically.
    ///
    /// AppView URL resolution (highest priority wins):
    /// 1. Runtime env override — CLI checks `OPAKE_APPVIEW_URL` after construction
    /// 2. Account config on PDS — fetched best-effort, user-configurable in settings
    /// 3. `DEFAULT_APPVIEW_URL` — baked in at compile time from `OPAKE_APPVIEW_URL` env var
    ///
    /// Pass `None` for the default account, or `Some(did)` for a specific one.
    pub async fn for_account(
        storage: S,
        did: Option<&str>,
        transport: T,
        mut rng: R,
        now: fn() -> String,
        now_micros: fn() -> u64,
    ) -> Result<Self, Error> {
        let config = storage.load_config().await?;

        let target_did = match did {
            Some(d) => d.to_string(),
            None => config
                .default_did
                .ok_or_else(|| Error::NotFound("no default account configured".into()))?,
        };

        let account = config
            .accounts
            .get(&target_did)
            .ok_or_else(|| Error::NotFound(format!("no account for {target_did}")))?;

        let session = storage.load_session(&target_did).await?;

        let identity = match storage.load_identity(&target_did).await {
            Ok(mut id) => {
                if id.ensure_signing_keys(&mut rng) {
                    let _ = storage.save_identity(&target_did, &id).await;
                }
                Some(id)
            }
            Err(_) => None,
        };

        let client = XrpcClient::with_session(transport, account.pds_url.clone(), session);
        let mut opake = Self::new(client, target_did, identity, rng, storage, now, now_micros);
        opake.appview_url = Some(DEFAULT_APPVIEW_URL.to_string());
        Ok(opake)
    }

    /// List all signed-in accounts from Storage.
    pub async fn list_accounts(storage: &S) -> Result<Vec<crate::storage::AccountInfo>, Error> {
        let config = storage.load_config().await?;
        Ok(config
            .accounts
            .iter()
            .map(|(did, entry)| crate::storage::AccountInfo {
                did: did.clone(),
                pds_url: entry.pds_url.clone(),
                handle: entry.handle.clone(),
                is_default: config.default_did.as_deref() == Some(did),
            })
            .collect())
    }

    // -- File context creation --

    /// Build a FileContext for the cabinet or a named workspace.
    ///
    /// Pass `None` for the personal cabinet, or `Some("workspace-name")`
    /// to resolve a workspace by name (fetches keyring from PDS, unwraps
    /// the group key).
    pub async fn file_context(
        &mut self,
        workspace_name: Option<&str>,
    ) -> Result<FileContext, Error> {
        match workspace_name {
            Some(name) => {
                let ws = self.resolve_workspace(name).await?;
                Ok(FileContext::Workspace(ws))
            }
            None => {
                let cabinet = Cabinet::from_identity(self.require_identity()?)?;
                Ok(FileContext::Cabinet(cabinet))
            }
        }
    }

    /// Build a cabinet FileContext.
    pub fn cabinet_context(&self) -> Result<FileContext, Error> {
        let cabinet = Cabinet::from_identity(self.require_identity()?)?;
        Ok(FileContext::Cabinet(cabinet))
    }

    /// Create a FileManager that borrows this Opake and the given context.
    ///
    /// The FileManager is the primary API for file operations. It dispatches
    /// based on the context (cabinet vs workspace) and handles encryption
    /// differences transparently.
    pub fn file_manager<'a>(&'a mut self, context: &'a FileContext) -> FileManager<'a, T, R, S> {
        FileManager {
            opake: self,
            context,
            last_proposals: Vec::new(),
            last_keyring_proposals: Vec::new(),
            last_document_proposals: Vec::new(),
        }
    }

    /// Create a WorkspaceAdmin for membership operations.
    ///
    /// Separate from FileManager because member management (invite, leave)
    /// is not a file operation.
    pub fn workspace_admin<'a>(
        &'a mut self,
        workspace: &'a Workspace,
    ) -> WorkspaceAdmin<'a, T, R, S> {
        WorkspaceAdmin {
            opake: self,
            workspace,
        }
    }

    // -- Workspace resolution --

    /// Resolve a workspace by name via the appview member-keyrings index.
    ///
    /// The appview indexes every keyring from the firehose and serves them
    /// via `/api/keyrings` filtered to ones where the caller is a member —
    /// which includes workspaces the caller owns (they're always a member
    /// of their own). That makes it the single source of truth for
    /// name → URI resolution regardless of who owns the keyring.
    ///
    /// Once a unique URI is picked, [`resolve_workspace_by_uri`] fetches
    /// the canonical record from the owner's PDS and unwraps the group
    /// key there — so the appview is only trusted for the name → URI map,
    /// not for the group key material.
    ///
    /// Limitation: workspace creation writes to the caller's PDS, which
    /// the appview indexes with some lag (seconds) through the firehose.
    /// A `workspace ls` immediately after `workspace create` may miss the
    /// new entry until Jetstream delivers the commit.
    pub async fn resolve_workspace(&mut self, name: &str) -> Result<Workspace, Error> {
        let identity = self.require_identity()?;
        let private_key = identity.private_key_bytes()?;

        let keyrings = self.discover_member_keyrings(None).await?;
        let matches: Vec<String> = keyrings
            .iter()
            .filter(|kr| {
                keyrings::decrypt_appview_keyring_name(kr, &self.did, &private_key).as_deref()
                    == Some(name)
            })
            .map(|kr| kr.uri.clone())
            .collect();

        match matches.as_slice() {
            [] => Err(Error::NotFound(format!("no keyring named {name:?}"))),
            [uri] => {
                let uri = uri.clone();
                self.resolve_workspace_by_uri(&uri).await
            }
            uris => Err(Error::AmbiguousName {
                name: name.to_string(),
                count: uris.len(),
                uris: uris.to_vec(),
            }),
        }
    }

    /// Resolve a workspace by keyring URI.
    ///
    /// Works for both own and foreign workspaces — detects ownership from
    /// the URI's authority and routes to the appropriate resolution path.
    /// The group key never leaves Rust; it's held in the returned Workspace.
    pub async fn resolve_workspace_by_uri(
        &mut self,
        keyring_uri: &str,
    ) -> Result<Workspace, Error> {
        let at_uri = atproto::parse_at_uri(keyring_uri)?;
        if at_uri.authority == self.did {
            // Own keyring — fetch with authenticated client
            let identity = self.require_identity()?;
            let private_key = identity.private_key_bytes()?;
            let entry = self
                .client
                .get_record(&self.did, &at_uri.collection, &at_uri.rkey)
                .await?;
            let keyring: crate::records::Keyring = serde_json::from_value(entry.value)?;
            let group_key = Self::unwrap_workspace_key(&keyring.members, &self.did, &private_key)?;
            let name = keyrings::decrypt_keyring_name_from_record(&keyring, &group_key)
                .unwrap_or_default();
            Ok(Workspace::from_keyring(
                keyring_uri.to_string(),
                name,
                None,
                self.did.clone(),
                group_key,
                keyring.rotation,
            ))
        } else {
            // Foreign keyring — resolve via public PDS endpoint
            self.resolve_foreign_workspace(keyring_uri).await
        }
    }

    /// Resolve a workspace from a foreign PDS by keyring URI.
    ///
    /// Cross-PDS — fetches the keyring record from the owner's PDS (public
    /// endpoint), unwraps the group key, decrypts metadata. Used for
    /// workspaces where the caller is a member but not the owner.
    pub async fn resolve_foreign_workspace(&self, keyring_uri: &str) -> Result<Workspace, Error> {
        let identity = self.require_identity()?;
        let private_key = identity.private_key_bytes()?;
        let at_uri = crate::atproto::parse_at_uri(keyring_uri)?;
        let owner_did = &at_uri.authority;

        // Resolve owner's PDS from their DID document
        let did_doc =
            crate::client::resolve_did_document(self.client.transport(), owner_did).await?;
        let owner_pds = crate::client::pds_from_did_document(&did_doc)?;

        // Fetch keyring record (public endpoint)
        let entry = crate::client::get_record_public(
            self.client.transport(),
            &owner_pds,
            owner_did,
            &at_uri.collection,
            &at_uri.rkey,
        )
        .await?;

        let keyring: crate::records::Keyring = serde_json::from_value(entry.value)?;
        let group_key = Self::unwrap_workspace_key(&keyring.members, &self.did, &private_key)?;

        // Decrypt metadata for the workspace name
        let name =
            keyrings::decrypt_keyring_name_from_record(&keyring, &group_key).unwrap_or_default();

        Ok(Workspace::from_keyring(
            keyring_uri.to_string(),
            name,
            None,
            owner_did.to_string(),
            group_key,
            keyring.rotation,
        ))
    }

    // -- Accessors --

    /// The caller's DID.
    pub fn did(&self) -> &str {
        &self.did
    }

    /// Mutable access to the XRPC client for external callers.
    pub fn client_mut(&mut self) -> &mut XrpcClient<T> {
        &mut self.client
    }

    /// The caller's identity, if one exists (may be absent pre-pairing).
    pub fn identity(&self) -> Option<&Identity> {
        self.identity.as_ref()
    }

    /// The caller's identity, or error if absent.
    ///
    /// Use this for operations that need encryption keys.
    pub fn require_identity(&self) -> Result<&Identity, Error> {
        self.identity.as_ref().ok_or_else(|| {
            Error::NotFound("no identity — generate keys or pair this device first".into())
        })
    }

    /// Current ISO 8601 timestamp from the platform clock.
    pub fn now(&self) -> String {
        (self.now_fn)()
    }

    /// Generate a TID (Timestamp ID) for use as a record rkey.
    pub(crate) fn generate_tid(&self) -> String {
        crate::tid::tid_from_micros((self.now_micros_fn)())
    }

    /// The current session, if any.
    pub fn session(&self) -> Option<&crate::client::Session> {
        self.client.session()
    }

    /// Apply a proactively refreshed session: update the in-memory client and
    /// persist to storage. Used by the SDK's token guard and the daemon worker.
    pub async fn persist_refreshed_session(
        &mut self,
        session: &crate::client::Session,
    ) -> Result<(), Error> {
        self.storage.save_session(&self.did, session).await?;
        self.client.set_session(session.clone());
        Ok(())
    }

    /// Persist the session to storage if it was refreshed since the last persist.
    pub(crate) async fn auto_persist_session(&self) -> Result<(), Error> {
        if self.client.session_refreshed() {
            if let Some(session) = self.client.session() {
                self.storage.save_session(&self.did, session).await?;
            }
        }
        Ok(())
    }

    /// Persist session if needed, then pass through the result.
    ///
    /// Use at the end of every public method that touches the PDS client.
    /// If the operation already failed, persist is best-effort (the
    /// operation error is more useful than a persist error).
    pub(crate) async fn signoff<V>(&self, result: Result<V, Error>) -> Result<V, Error> {
        if result.is_ok() {
            self.auto_persist_session().await?;
        } else {
            let _ = self.auto_persist_session().await;
        }
        result
    }

    /// Create a record on the caller's PDS.
    ///
    /// Low-level passthrough for one-off writes that don't fit the
    /// FileManager contract (e.g., pending shares, account config).
    pub async fn create_record(
        &mut self,
        collection: &str,
        record: &impl serde::Serialize,
    ) -> Result<crate::client::RecordRef, Error> {
        let result = self.client.create_record(collection, record).await;
        self.signoff(result).await
    }

    // -- Workspace management (keyring operations) --

    /// Create a new workspace. Returns `(keyring_uri, key)`.
    pub async fn create_workspace(
        &mut self,
        name: &str,
        description: Option<&str>,
    ) -> Result<(String, ContentKey), Error> {
        let identity = self.require_identity()?;
        let pubkey = identity.public_key_bytes()?;
        let now = self.now();
        let result = keyrings::create_keyring(
            &mut self.client,
            &CreateKeyringParams {
                name,
                description,
                owner_did: &self.did,
                owner_public_key: &pubkey,
                created_at: &now,
            },
            &mut self.rng,
        )
        .await?;
        self.auto_persist_session().await?;
        Ok(result)
    }

    /// Sync all workspaces: apply proposals (owned) + cleanup stale records (member).
    ///
    /// Discovers all workspaces via the AppView (includes both owned and member
    /// workspaces). For each:
    /// - Syncs the tree from AppView
    /// - Cleans up the caller's own applied proposal records from their PDS
    /// - Applies pending proposals if the caller is the owner
    ///
    /// Returns the total number of proposals applied across all workspaces.
    pub async fn sync_owned_workspaces(&mut self) -> Result<usize, Error> {
        let results = self.sync_owned_workspaces_detailed().await?;
        Ok(results.iter().map(|r| r.proposals_applied).sum())
    }

    /// Sync all workspaces with per-workspace result visibility.
    ///
    /// Discovers workspaces via AppView, syncs each one, returns a result per
    /// workspace. Per-workspace errors are captured (not propagated) so one
    /// failing workspace doesn't block the rest.
    pub async fn sync_owned_workspaces_detailed(
        &mut self,
    ) -> Result<Vec<crate::daemon::WorkspaceSyncResult>, Error> {
        log::trace!("sync: discovering workspaces for {}", self.did);
        let appview_keyrings = self.discover_member_keyrings(None).await?;
        log::trace!("sync: found {} workspaces", appview_keyrings.len());
        let identity = self.require_identity()?;
        let private_key = identity.private_key_bytes()?;

        let mut results = Vec::with_capacity(appview_keyrings.len());
        for kr in &appview_keyrings {
            results.push(self.sync_single_workspace(kr, &private_key).await);
        }

        self.auto_persist_session().await?;
        Ok(results)
    }

    /// Sync a single workspace identified by its keyring URI.
    ///
    /// Fetches all member keyrings from the appview (same as the full sync),
    /// finds the target, and syncs only that one. Returns `None` if the
    /// keyring URI wasn't found in the member list.
    pub async fn sync_workspace_by_uri(
        &mut self,
        keyring_uri: &str,
    ) -> Result<Option<crate::daemon::WorkspaceSyncResult>, Error> {
        let appview_keyrings = self.discover_member_keyrings(None).await?;
        let target = appview_keyrings.iter().find(|kr| kr.uri == keyring_uri);
        let Some(kr) = target else { return Ok(None) };

        let identity = self.require_identity()?;
        let private_key = identity.private_key_bytes()?;
        let result = self.sync_single_workspace(kr, &private_key).await;
        self.auto_persist_session().await?;
        Ok(Some(result))
    }

    /// Sync a single workspace: cleanup + apply proposals.
    ///
    /// Captures all errors into `WorkspaceSyncResult::error` instead of
    /// propagating — the caller can continue with remaining workspaces.
    async fn sync_single_workspace(
        &mut self,
        kr: &crate::client::AppviewKeyring,
        private_key: &crate::crypto::X25519PrivateKey,
    ) -> crate::daemon::WorkspaceSyncResult {
        use crate::daemon::WorkspaceSyncResult;

        let is_owner = kr.owner_did == self.did;
        log::trace!("sync: processing {} (owner={is_owner})", kr.uri);

        let members: Vec<crate::records::KeyringMember> = kr
            .members
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();

        let group_key = match Self::unwrap_workspace_key(&members, &self.did, private_key) {
            Ok(k) => k,
            Err(e) => {
                return WorkspaceSyncResult {
                    keyring_uri: kr.uri.clone(),
                    is_owner,
                    proposals_applied: 0,
                    proposals_cleaned_up: 0,
                    error: Some(format!("key unwrap: {e}")),
                };
            }
        };

        let workspace = crate::workspace::Workspace::from_keyring(
            kr.uri.clone(),
            String::new(),
            None,
            kr.owner_did.clone(),
            group_key,
            kr.rotation,
        );
        let ctx = crate::manager::FileContext::Workspace(workspace);
        let mut mgr = self.file_manager(&ctx);
        let tree = match mgr.load_tree().await {
            Ok(t) => t,
            Err(e) => {
                return WorkspaceSyncResult {
                    keyring_uri: kr.uri.clone(),
                    is_owner,
                    proposals_applied: 0,
                    proposals_cleaned_up: 0,
                    error: Some(format!("tree sync: {e}")),
                };
            }
        };

        // Cleanup + apply directory proposals
        let cleaned_up = mgr.cleanup_own_applied_proposals(&tree).await;
        let mut applied = 0usize;
        let mut error: Option<String> = None;

        match mgr.apply_pending_proposals(&tree).await {
            Ok(n) => applied += n,
            Err(e) => {
                log::warn!("proposal apply failed for {}: {e}", kr.uri);
                error = Some(format!("directory proposals: {e}"));
            }
        }

        // Extract proposal data before dropping mgr (releases self borrow)
        let keyring_proposals = mgr.last_keyring_proposals().to_vec();
        let doc_proposals = mgr.last_document_proposals().to_vec();
        drop(mgr);

        // Cleanup own applied keyring proposals
        let member_dids: std::collections::HashSet<&str> =
            members.iter().map(|m| m.did()).collect();
        self.cleanup_own_applied_keyring_proposals(&keyring_proposals, &member_dids)
            .await;

        // Owner-only: document + keyring proposals
        if is_owner && !doc_proposals.is_empty() {
            match self.apply_document_proposals(&doc_proposals).await {
                Ok(n) => applied += n,
                Err(e) => {
                    log::warn!("document proposal apply failed for {}: {e}", kr.uri);
                    error = Some(format!("document proposals: {e}"));
                }
            }
        }

        if is_owner && !keyring_proposals.is_empty() {
            match self
                .apply_keyring_proposals(&kr.uri, &keyring_proposals)
                .await
            {
                Ok(n) => applied += n,
                Err(e) => {
                    log::warn!("keyring proposal apply failed for {}: {e}", kr.uri);
                    error = Some(format!("keyring proposals: {e}"));
                }
            }
        }

        WorkspaceSyncResult {
            keyring_uri: kr.uri.clone(),
            is_owner,
            proposals_applied: applied,
            proposals_cleaned_up: cleaned_up,
            error,
        }
    }

    /// Apply keyring update proposals for an owned workspace.
    ///
    /// Fetches the current keyring, applies each proposal (addMember, removeMember,
    /// rename, updateDescription, updateRole), then writes the updated record back.
    async fn apply_keyring_proposals(
        &mut self,
        keyring_uri: &str,
        proposals: &[crate::client::KeyringProposal],
    ) -> Result<usize, Error> {
        use crate::crypto;
        use crate::records::{self, keyring_update};
        use base64::Engine;

        let at_uri = atproto::parse_at_uri(keyring_uri)?;
        let entry = self
            .client
            .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
            .await?;
        let mut keyring: records::Keyring = serde_json::from_value(entry.value)?;
        records::check_version(keyring.opake_version)?;

        // Derive identity + group key once for all proposals that need key wrapping.
        // Mutable because removeMember/leave rotates the key — subsequent proposals
        // must use the rotated key.
        let identity = self.require_identity()?;
        let private_key = identity.private_key_bytes()?;
        let mut group_key = Self::unwrap_workspace_key(&keyring.members, &self.did, &private_key)?;

        let mut applied = 0;

        for p in proposals {
            match p.action_type.as_str() {
                keyring_update::ACTION_ADD_MEMBER => {
                    let Some(ref did) = p.member_did else {
                        continue;
                    };
                    if keyring.members.iter().any(|m| m.did() == did) {
                        continue;
                    }
                    let Some(ref pk_b64) = p.member_public_key else {
                        continue;
                    };
                    let pk_bytes = match base64::engine::general_purpose::STANDARD.decode(pk_b64) {
                        Ok(b) => b,
                        Err(_) => continue,
                    };
                    let pubkey: crypto::X25519PublicKey = match pk_bytes.try_into() {
                        Ok(k) => k,
                        Err(_) => continue,
                    };
                    let role = p
                        .role
                        .as_deref()
                        .and_then(|r| r.parse::<records::Role>().ok())
                        .unwrap_or(records::Role::Editor);

                    let wrapped = crypto::wrap_key(&group_key, &pubkey, did, &mut self.rng)?;
                    keyring.members.push(records::KeyringMember {
                        wrapped_key: wrapped,
                        role,
                    });
                    applied += 1;
                    log::info!("keyring-sync: added member {did}");
                }
                keyring_update::ACTION_REMOVE_MEMBER | keyring_update::ACTION_LEAVE => {
                    // For leave, the author IS the member being removed
                    let did = if p.action_type == keyring_update::ACTION_LEAVE {
                        &p.author_did
                    } else {
                        match p.member_did.as_ref() {
                            Some(d) => d,
                            None => continue,
                        }
                    };
                    let before = keyring.members.len();
                    keyring.members.retain(|m| m.did() != did);
                    if keyring.members.len() < before {
                        // Remaining members need the old key to decrypt pre-rotation
                        // documents. Archive AFTER retain so the removed member is excluded.
                        keyring.key_history.push(records::KeyHistoryEntry {
                            rotation: keyring.rotation,
                            members: keyring.members.clone(),
                        });

                        // New group key requires each member's current public key
                        let mut remaining_pubkeys = Vec::with_capacity(keyring.members.len());
                        for member in &keyring.members {
                            let resolved = self.resolve_identity(member.did()).await?;
                            remaining_pubkeys.push((member.did().to_string(), resolved.public_key));
                        }

                        let did_members: Vec<crypto::DidMember<'_>> = remaining_pubkeys
                            .iter()
                            .map(|(d, pk)| crypto::DidMember {
                                did: d.as_str(),
                                public_key: pk,
                            })
                            .collect();

                        let (new_key, new_wrapped) =
                            crypto::create_group_key(&did_members, &mut self.rng)?;

                        // Roles must survive re-wrapping (new WrappedKeys lose the association)
                        let role_map: std::collections::HashMap<&str, records::Role> =
                            keyring.members.iter().map(|m| (m.did(), m.role)).collect();

                        keyring.members = new_wrapped
                            .into_iter()
                            .map(|wk| records::KeyringMember {
                                role: role_map
                                    .get(wk.did.as_str())
                                    .copied()
                                    .unwrap_or(records::Role::Editor),
                                wrapped_key: wk,
                            })
                            .collect();

                        // Metadata is encrypted under the group key — must re-encrypt
                        let metadata: crypto::KeyringMetadata =
                            crypto::decrypt_metadata(&group_key, &keyring.encrypted_metadata)?;
                        keyring.encrypted_metadata =
                            crypto::encrypt_metadata(&new_key, &metadata, &mut self.rng)?;

                        keyring.rotation += 1;
                        group_key = new_key;
                        applied += 1;
                        log::info!(
                            "keyring-sync: removed member {did}, rotated key to rotation {}",
                            keyring.rotation
                        );
                    }
                }
                keyring_update::ACTION_RENAME | keyring_update::ACTION_UPDATE_DESCRIPTION => {
                    if let Some(ref em_value) = p.encrypted_metadata {
                        if let Ok(em) =
                            serde_json::from_value::<records::EncryptedMetadata>(em_value.clone())
                        {
                            keyring.encrypted_metadata = em;
                            applied += 1;
                            log::info!("keyring-sync: applied metadata update");
                        }
                    }
                }
                keyring_update::ACTION_UPDATE_ROLE => {
                    let Some(ref did) = p.member_did else {
                        continue;
                    };
                    let Some(ref role_str) = p.role else {
                        continue;
                    };
                    let Ok(role) = role_str.parse::<records::Role>() else {
                        continue;
                    };
                    if let Some(member) = keyring.members.iter_mut().find(|m| m.did() == did) {
                        member.role = role;
                        applied += 1;
                        log::info!("keyring-sync: updated role for {did} to {role_str}");
                    }
                }
                _ => {}
            }
        }

        if applied > 0 {
            keyring.modified_at = Some(self.now());
            self.client
                .put_record(KEYRING_COLLECTION, &at_uri.rkey, &keyring)
                .await?;
            log::info!(
                "keyring-sync: applied {applied} keyring proposals for {}",
                keyring_uri
            );
        }

        Ok(applied)
    }

    /// Download a blob from a foreign PDS and re-host it on the caller's PDS.
    async fn rehost_blob(
        &mut self,
        source_pds: &str,
        source_did: &str,
        blob: &crate::atproto::BlobRef,
    ) -> Result<crate::atproto::BlobRef, Error> {
        let data = crate::client::get_blob_public(
            self.client.transport(),
            source_pds,
            source_did,
            &blob.reference.cid,
        )
        .await?;
        self.client.upload_blob(data, &blob.mime_type).await
    }

    /// Apply document update proposals for an owned workspace.
    ///
    /// For each pending document update from a workspace member:
    /// 1. Fetch the full documentUpdate record from the proposer's PDS
    /// 2. Apply the update (re-host blob, update document record)
    /// 3. Proposer cleanup is handled separately
    ///
    /// Only the workspace owner processes these — they own the document records.
    async fn apply_document_proposals(
        &mut self,
        proposals: &[crate::client::DocumentProposal],
    ) -> Result<usize, Error> {
        let mut applied = 0;

        for p in proposals {
            // Skip our own proposals (owner doesn't propose updates to their own docs)
            if p.author_did == self.did {
                continue;
            }

            let result = self.apply_single_document_proposal(p).await;

            match result {
                Ok(()) => {
                    applied += 1;
                    log::info!(
                        "document-sync: applied update {} for document {}",
                        p.uri,
                        p.document_uri
                    );
                }
                Err(e) => {
                    log::warn!(
                        "document-sync: failed to apply update {} for {}: {e}",
                        p.uri,
                        p.document_uri
                    );
                }
            }
        }

        if applied > 0 {
            self.auto_persist_session().await?;
        }

        Ok(applied)
    }

    /// Apply a single document update proposal.
    ///
    /// Fetches the full record from the proposer's PDS, downloads the blob,
    /// re-hosts it on the owner's PDS, and updates the document record.
    async fn apply_single_document_proposal(
        &mut self,
        proposal: &crate::client::DocumentProposal,
    ) -> Result<(), Error> {
        use crate::client::get_record_public;
        use crate::records;

        let doc_at_uri = atproto::parse_at_uri(&proposal.document_uri)?;
        let update_at_uri = atproto::parse_at_uri(&proposal.uri)?;

        // Resolve proposer's PDS
        let proposer_identity = self.resolve_identity(&proposal.author_did).await?;
        let proposer_pds = &proposer_identity.pds_url;

        // Fetch the full documentUpdate record from the proposer's PDS
        let update_entry = get_record_public(
            self.client.transport(),
            proposer_pds,
            &update_at_uri.authority,
            &update_at_uri.collection,
            &update_at_uri.rkey,
        )
        .await?;

        let update_record: records::DocumentUpdateRecord =
            serde_json::from_value(update_entry.value)?;
        records::check_version(update_record.opake_version)?;

        // Fetch the current document record from the owner's PDS
        let doc_entry = self
            .client
            .get_record(
                &doc_at_uri.authority,
                &doc_at_uri.collection,
                &doc_at_uri.rkey,
            )
            .await?;
        let mut document: records::Document = serde_json::from_value(doc_entry.value)?;
        records::check_version(document.opake_version)?;

        let now = self.now();

        match update_record.update {
            records::DocumentUpdate::UpdateContent { blob, .. } => {
                document.blob = self
                    .rehost_blob(proposer_pds, &proposal.author_did, &blob)
                    .await?;
            }
            records::DocumentUpdate::UpdateMetadata {
                encrypted_metadata, ..
            } => {
                document.encrypted_metadata = encrypted_metadata;
            }
            records::DocumentUpdate::Supersede {
                blob,
                encrypted_metadata,
                ..
            } => {
                document.blob = self
                    .rehost_blob(proposer_pds, &proposal.author_did, &blob)
                    .await?;
                document.encrypted_metadata = encrypted_metadata;
            }
        }
        document.modified_at = Some(now);

        // Write updated document back
        self.client
            .put_record(&doc_at_uri.collection, &doc_at_uri.rkey, &document)
            .await?;

        Ok(())
    }

    /// Clean up the caller's own keyring proposals that have been applied.
    ///
    /// Runs for all members (not just the owner). Compares each of the caller's
    /// own proposals against the current keyring member list. If the proposal's
    /// effect is reflected in the keyring state, deletes the proposal record
    /// from the caller's PDS so the AppView drops it from its index.
    ///
    /// Metadata proposals (rename, updateDescription) are skipped — verifying
    /// encrypted content isn't practical here. They're idempotent, so the owner
    /// just skips them on re-processing.
    async fn cleanup_own_applied_keyring_proposals(
        &mut self,
        proposals: &[crate::client::KeyringProposal],
        member_dids: &std::collections::HashSet<&str>,
    ) -> usize {
        use crate::client::ApplyWriteOp;
        use crate::records::keyring_update;

        let own_proposals: Vec<_> = proposals
            .iter()
            .filter(|p| p.author_did == self.did)
            .collect();

        if own_proposals.is_empty() {
            return 0;
        }

        let mut delete_ops: Vec<ApplyWriteOp> = Vec::new();

        for p in &own_proposals {
            let applied = match p.action_type.as_str() {
                keyring_update::ACTION_ADD_MEMBER => p
                    .member_did
                    .as_ref()
                    .is_some_and(|did| member_dids.contains(&did.as_str())),
                keyring_update::ACTION_REMOVE_MEMBER => p
                    .member_did
                    .as_ref()
                    .is_some_and(|did| !member_dids.contains(&did.as_str())),
                keyring_update::ACTION_LEAVE => !member_dids.contains(&p.author_did.as_str()),
                keyring_update::ACTION_UPDATE_ROLE => {
                    // Role is on the keyring record, which we don't have here.
                    // Skip — harmless to re-process.
                    false
                }
                // rename, updateDescription — can't verify without decrypting
                _ => false,
            };

            if applied {
                log::trace!(
                    "keyring-cleanup: proposal {} ({}) applied, will delete",
                    p.uri,
                    p.action_type
                );
                if let Ok(at_uri) = atproto::parse_at_uri(&p.uri) {
                    delete_ops.push(ApplyWriteOp::Delete {
                        collection: at_uri.collection.clone(),
                        rkey: at_uri.rkey.clone(),
                    });
                }
            }
        }

        if delete_ops.is_empty() {
            return 0;
        }

        let count = delete_ops.len();
        match self.client.apply_writes(&delete_ops).await {
            Ok(()) => {
                log::info!(
                    "keyring-cleanup: deleted {count} applied proposal records from own PDS"
                );
                count
            }
            Err(e) => {
                log::warn!("keyring-cleanup: failed to delete applied proposals: {e}");
                0
            }
        }
    }

    /// Add a member to a workspace.
    ///
    /// Owner: applies directly. Non-owner manager: creates a keyringUpdate
    /// proposal carrying the new member's DID and public key.
    pub async fn add_workspace_member(
        &mut self,
        keyring_uri: &str,
        key: &ContentKey,
        member_did: &str,
        member_public_key: &X25519PublicKey,
        role: Role,
    ) -> Result<MutationOutcome, Error> {
        let owner_did = atproto::parse_at_uri(keyring_uri)?.authority.to_string();
        let is_owner = owner_did == self.did();
        let now = self.now();

        if is_owner {
            keyrings::add_member(
                &mut self.client,
                &AddMemberParams {
                    keyring_uri,
                    group_key: key,
                    new_member_did: member_did,
                    new_member_public_key: member_public_key,
                    role,
                    modified_at: &now,
                },
                &mut self.rng,
            )
            .await?;
            self.auto_persist_session().await?;
            Ok(MutationOutcome::Applied)
        } else {
            let update = KeyringUpdateRecord::add_member(
                keyring_uri.to_string(),
                member_did.to_string(),
                member_public_key.to_vec(),
                role.to_string(),
                now,
            );
            self.client
                .create_record(KEYRING_UPDATE_COLLECTION, &update)
                .await?;
            self.auto_persist_session().await?;
            Ok(MutationOutcome::Proposed {
                update_uri: keyring_uri.to_string(),
            })
        }
    }

    /// Leave a workspace. Writes a keyringUpdate with actionType "leave".
    ///
    /// The AppView handles visibility immediately (stops listing the workspace).
    /// The owner's daemon processes key rotation asynchronously.
    pub async fn leave_workspace(&mut self, keyring_uri: &str) -> Result<String, Error> {
        let now = self.now();
        let record = KeyringUpdateRecord::leave(keyring_uri.to_string(), now);
        let record_ref = self
            .client
            .create_record(KEYRING_UPDATE_COLLECTION, &record)
            .await?;
        self.auto_persist_session().await?;
        Ok(record_ref.uri)
    }

    /// Remove a member from a workspace.
    ///
    /// Owner: rotates the group key, re-wraps to remaining members, returns
    /// `(new_group_key, new_rotation)`. Non-owner: creates a keyringUpdate
    /// proposal; returns `None` since no key rotation happened locally.
    pub async fn remove_workspace_member(
        &mut self,
        keyring_uri: &str,
        group_key: &ContentKey,
        member_did: &str,
    ) -> Result<(Option<(ContentKey, u64)>, MutationOutcome), Error> {
        let at_uri = atproto::parse_at_uri(keyring_uri)?;
        let owner_did = at_uri.authority.to_string();
        let is_owner = owner_did == self.did();
        let now = self.now();

        if is_owner {
            let entry = self
                .client
                .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
                .await?;
            let keyring: crate::records::Keyring = serde_json::from_value(entry.value)?;
            crate::records::check_version(keyring.opake_version)?;

            let remaining_dids: Vec<&str> = keyring
                .members
                .iter()
                .filter(|m| m.did() != member_did)
                .map(|m| m.did())
                .collect();

            let mut remaining_pubkeys = Vec::new();
            for did in &remaining_dids {
                let identity = self.resolve_identity(did).await?;
                remaining_pubkeys.push(identity.public_key);
            }

            let remaining_keys: Vec<DidMember<'_>> = remaining_dids
                .iter()
                .enumerate()
                .map(|(i, did)| DidMember {
                    did,
                    public_key: &remaining_pubkeys[i],
                })
                .collect();

            let workspace = Workspace {
                uri: keyring_uri.to_string(),
                name: String::new(),
                description: None,
                owner_did,
                key: group_key.clone(),
                rotation: keyring.rotation,
            };
            let mut admin = self.workspace_admin(&workspace);
            let result = admin.remove_member(member_did, &remaining_keys).await?;
            Ok((Some(result), MutationOutcome::Applied))
        } else {
            let update = KeyringUpdateRecord::remove_member(
                keyring_uri.to_string(),
                member_did.to_string(),
                now,
            );
            self.client
                .create_record(KEYRING_UPDATE_COLLECTION, &update)
                .await?;
            self.auto_persist_session().await?;
            Ok((
                None,
                MutationOutcome::Proposed {
                    update_uri: keyring_uri.to_string(),
                },
            ))
        }
    }

    /// Update workspace metadata (name, description).
    ///
    /// Owner: applies directly. Non-owner: encrypts the new metadata and
    /// creates a keyringUpdate proposal.
    pub async fn update_workspace_metadata(
        &mut self,
        keyring_uri: &str,
        group_key: &ContentKey,
        name: Option<&str>,
        description: Option<&str>,
        icon: Option<&str>,
    ) -> Result<MutationOutcome, Error> {
        use crate::crypto::{self, KeyringMetadata};
        use crate::records;

        let at_uri = atproto::parse_at_uri(keyring_uri)?;
        let owner_did = at_uri.authority.to_string();
        let is_owner = owner_did == self.did();

        // Both paths need the current metadata to patch it
        let entry = self
            .client
            .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
            .await?;
        let mut keyring: records::Keyring = serde_json::from_value(entry.value)?;
        records::check_version(keyring.opake_version)?;

        let mut metadata: KeyringMetadata =
            crypto::decrypt_metadata(group_key, &keyring.encrypted_metadata)?;

        if let Some(n) = name {
            metadata.name = n.to_string();
        }
        if let Some(d) = description {
            metadata.description = if d.is_empty() {
                None
            } else {
                Some(d.to_string())
            };
        }
        if let Some(i) = icon {
            metadata.icon = if i.is_empty() {
                None
            } else {
                Some(i.to_string())
            };
        }

        let new_encrypted = crypto::encrypt_metadata(group_key, &metadata, &mut self.rng)?;

        if is_owner {
            keyring.encrypted_metadata = new_encrypted;
            keyring.modified_at = Some(self.now());

            self.client
                .put_record(KEYRING_COLLECTION, &at_uri.rkey, &keyring)
                .await?;
            self.auto_persist_session().await?;
            Ok(MutationOutcome::Applied)
        } else {
            let now = self.now();
            let has_name_change = name.is_some();
            let update = if has_name_change {
                KeyringUpdateRecord::rename(keyring_uri.to_string(), new_encrypted, now)
            } else {
                KeyringUpdateRecord::update_description(keyring_uri.to_string(), new_encrypted, now)
            };
            self.client
                .create_record(KEYRING_UPDATE_COLLECTION, &update)
                .await?;
            self.auto_persist_session().await?;
            Ok(MutationOutcome::Proposed {
                update_uri: keyring_uri.to_string(),
            })
        }
    }

    /// Update a workspace member's role.
    ///
    /// Owner: modifies the keyring record directly. Non-owner: creates a
    /// keyringUpdate proposal.
    pub async fn update_member_role(
        &mut self,
        keyring_uri: &str,
        member_did: &str,
        new_role: Role,
    ) -> Result<MutationOutcome, Error> {
        let at_uri = atproto::parse_at_uri(keyring_uri)?;
        let owner_did = at_uri.authority.to_string();
        let is_owner = owner_did == self.did();
        let now = self.now();

        if is_owner {
            let entry = self
                .client
                .get_record(&at_uri.authority, &at_uri.collection, &at_uri.rkey)
                .await?;
            let mut keyring: crate::records::Keyring = serde_json::from_value(entry.value)?;
            crate::records::check_version(keyring.opake_version)?;

            let found = keyring.members.iter_mut().find(|m| m.did() == member_did);
            match found {
                Some(member) => member.role = new_role,
                None => {
                    return Err(Error::InvalidRecord(format!(
                        "{member_did} is not a member of this keyring"
                    )));
                }
            }
            keyring.modified_at = Some(now);

            self.client
                .put_record(KEYRING_COLLECTION, &at_uri.rkey, &keyring)
                .await?;
            self.auto_persist_session().await?;
            Ok(MutationOutcome::Applied)
        } else {
            let update = KeyringUpdateRecord::update_role(
                keyring_uri.to_string(),
                member_did.to_string(),
                new_role.to_string(),
                now,
            );
            self.client
                .create_record(KEYRING_UPDATE_COLLECTION, &update)
                .await?;
            self.auto_persist_session().await?;
            Ok(MutationOutcome::Proposed {
                update_uri: keyring_uri.to_string(),
            })
        }
    }

    // -- Invitations --

    /// Create a workspace invitation with a random token.
    /// Returns `(invitation_uri, token)`.
    pub async fn create_invitation(
        &mut self,
        keyring_uri: &str,
        role: &str,
    ) -> Result<(String, String), Error> {
        let token = self.generate_token();
        let now = self.now();
        let record = Invitation::workspace(keyring_uri.to_string(), role, token.clone(), now);
        let record_ref = self
            .client
            .create_record(INVITATION_COLLECTION, &record)
            .await?;
        self.auto_persist_session().await?;
        Ok((record_ref.uri, token))
    }

    /// List all invitations on the caller's PDS.
    pub async fn list_invitations(&mut self) -> Result<Vec<(String, Invitation)>, Error> {
        let page = self
            .client
            .list_records(INVITATION_COLLECTION, Some(100), None)
            .await?;
        let mut invitations = Vec::new();
        for entry in page.records {
            let invitation: Invitation = serde_json::from_value(entry.value)?;
            invitations.push((entry.uri, invitation));
        }
        self.auto_persist_session().await?;
        Ok(invitations)
    }

    /// Delete an invitation record (revoke).
    pub async fn revoke_invitation(&mut self, invitation_uri: &str) -> Result<(), Error> {
        let at_uri = atproto::parse_at_uri(invitation_uri)?;
        self.client
            .delete_record(&at_uri.collection, &at_uri.rkey)
            .await?;
        self.auto_persist_session().await?;
        Ok(())
    }

    /// Accept an invitation by writing an acceptance record to the caller's PDS.
    /// Returns the acceptance record URI.
    pub async fn accept_invitation(&mut self, invitation_uri: &str) -> Result<String, Error> {
        let now = self.now();
        let record = InvitationAcceptance::new(invitation_uri.to_string(), now);
        let record_ref = self
            .client
            .create_record(INVITATION_ACCEPTANCE_COLLECTION, &record)
            .await?;
        self.auto_persist_session().await?;
        Ok(record_ref.uri)
    }

    /// Generate a random URL-safe token for invitations.
    fn generate_token(&mut self) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        let mut bytes = [0u8; 24];
        self.rng.fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Download and decrypt a file using a grant (cross-PDS, recipient side).
    ///
    /// The grant and document live on the owner's PDS. All fetches are
    /// unauthenticated public endpoint calls. Returns `(filename, plaintext)`.
    pub async fn download_from_grant(&self, grant_uri: &str) -> Result<(String, Vec<u8>), Error> {
        let private_key = self.require_identity()?.private_key_bytes()?;
        crate::documents::download_from_grant(self.client.transport(), &private_key, grant_uri)
            .await
    }

    /// Resolve an incoming grant's document metadata without downloading the blob.
    ///
    /// Cross-PDS — fetches grant + document records from the owner's PDS,
    /// unwraps the content key, decrypts metadata. Returns `(filename, metadata)`.
    pub async fn resolve_grant_metadata(
        &self,
        grant_uri: &str,
    ) -> Result<(String, crate::crypto::DocumentMetadata), Error> {
        let private_key = self.require_identity()?.private_key_bytes()?;
        crate::documents::resolve_grant_metadata(self.client.transport(), &private_key, grant_uri)
            .await
    }

    /// Unwrap a workspace key from keyring member data (pure crypto, no network).
    pub fn unwrap_workspace_key(
        members: &[crate::records::KeyringMember],
        did: &str,
        private_key: &crate::crypto::X25519PrivateKey,
    ) -> Result<ContentKey, Error> {
        let member = members
            .iter()
            .find(|m| m.did() == did)
            .ok_or_else(|| Error::NotFound(format!("no member entry for DID {did}")))?;
        crate::crypto::unwrap_key(&member.wrapped_key, private_key)
    }

    // -- AppView helpers --

    /// Set the AppView URL if not already configured (e.g. from a build-time default).
    pub fn set_default_appview_url(&mut self, url: &str) {
        if self.appview_url.is_none() {
            self.appview_url = Some(url.to_string());
        }
    }

    /// Override the AppView URL unconditionally (runtime env var override).
    pub fn set_appview_url(&mut self, url: String) {
        self.appview_url = Some(url);
    }

    /// Resolve the appview URL: cached config → caller default → error.
    /// Resolve the appview URL to use for a request.
    ///
    /// Returns the URL stored on this Opake instance (loaded from config
    /// during `init`), falling back to the provided `default` if no URL
    /// is stored. Returns `NotFound` if neither source has a URL.
    ///
    /// This is the shared helper behind every appview-touching method
    /// (`request_sse_token`, `list_inbox`, `discover_member_keyrings`,
    /// etc.) and also used by WASM bindings that need to auto-resolve
    /// the URL before starting long-lived tasks like the SSE consumer.
    pub fn resolve_appview_url(&self, default: Option<&str>) -> Result<String, Error> {
        self.appview_url
            .clone()
            .or_else(|| default.map(|s| s.to_string()))
            .ok_or_else(|| {
                Error::NotFound(
                    "no appview URL — set VITE_APPVIEW_URL or configure in settings".into(),
                )
            })
    }

    // -- SSE token (for EventSource auth) --

    /// Request a short-lived SSE token from the AppView.
    ///
    /// The token is passed as a query parameter to the SSE endpoint,
    /// sidestepping EventSource's inability to send custom headers.
    pub async fn request_sse_token(
        &mut self,
        default_appview_url: Option<&str>,
    ) -> Result<String, Error> {
        let identity = self.require_identity()?;
        let signing_key = identity
            .signing_key_bytes()?
            .ok_or_else(|| Error::Auth("no signing key for SSE token request".into()))?;

        let url = self.resolve_appview_url(default_appview_url)?;
        crate::client::request_sse_token(self.client.transport(), &url, &self.did, &signing_key)
            .await
    }

    // -- Inbox (incoming grants via AppView) --

    /// Fetch all incoming grants from the AppView.
    ///
    /// Returns an empty list if no appview URL is configured or no signing key exists.
    pub async fn list_inbox(
        &mut self,
        default_appview_url: Option<&str>,
    ) -> Result<Vec<crate::client::InboxGrant>, Error> {
        let identity = match self.identity.as_ref() {
            Some(id) => id,
            None => return Ok(vec![]),
        };
        let signing_key = match identity.signing_key_bytes()? {
            Some(k) => k,
            None => return Ok(vec![]),
        };

        let url = self.resolve_appview_url(default_appview_url)?;

        crate::client::fetch_inbox_all(self.client.transport(), &url, &self.did, &signing_key).await
    }

    /// Fetch workspace documents from the AppView.
    ///
    /// Returns an empty list if no appview URL is configured or no signing key exists.
    pub async fn list_workspace_documents(
        &mut self,
        keyring_uri: &str,
        default_appview_url: Option<&str>,
    ) -> Result<Vec<crate::client::WorkspaceDocument>, Error> {
        let identity = match self.identity.as_ref() {
            Some(id) => id,
            None => return Ok(vec![]),
        };
        let signing_key = match identity.signing_key_bytes()? {
            Some(k) => k,
            None => return Ok(vec![]),
        };

        let url = self.resolve_appview_url(default_appview_url)?;

        crate::client::fetch_workspace_documents(
            self.client.transport(),
            &url,
            &self.did,
            &signing_key,
            keyring_uri,
        )
        .await
    }

    /// Fetch all keyrings the user is a member of, with full record data.
    ///
    /// Returns an empty list if no appview URL is configured or no signing key exists.
    pub async fn discover_member_keyrings(
        &mut self,
        default_appview_url: Option<&str>,
    ) -> Result<Vec<crate::client::AppviewKeyring>, Error> {
        let identity = match self.identity.as_ref() {
            Some(id) => id,
            None => return Ok(vec![]),
        };
        let signing_key = match identity.signing_key_bytes()? {
            Some(k) => k,
            None => return Ok(vec![]),
        };

        let url = self.resolve_appview_url(default_appview_url)?;

        crate::client::fetch_member_keyrings(self.client.transport(), &url, &self.did, &signing_key)
            .await
    }

    // -- Sharing (pending shares) --

    /// List pending (queued) outgoing shares.
    pub async fn list_pending_shares(
        &mut self,
    ) -> Result<Vec<crate::sharing::PendingShareEntry>, Error> {
        let result = crate::sharing::list_pending_shares(&mut self.client).await;
        self.signoff(result).await
    }

    /// Cancel a pending share by AT-URI.
    pub async fn cancel_pending_share(&mut self, uri: &str) -> Result<(), Error> {
        let result = crate::sharing::cancel_pending_share(&mut self.client, uri).await;
        self.signoff(result).await
    }

    /// Retry all pending shares (resolve recipients, create grants for those now available).
    ///
    /// Needs a separate transport for cross-PDS recipient resolution (can't
    /// borrow the client's transport while mutating the client).
    pub async fn retry_pending_shares(
        &mut self,
        resolver_transport: &impl Transport,
    ) -> Result<crate::sharing::RetryResult, Error> {
        let identity = self.require_identity()?;
        let private_key = identity.private_key_bytes()?;
        let now = crate::client::time::unix_now();
        let pds_url = self.client.base_url().to_owned();
        let params = crate::sharing::RetryParams {
            caller_pds_url: &pds_url,
            owner_did: &self.did,
            owner_private_key: &private_key,
            now,
            ttl_seconds: crate::sharing::DEFAULT_PENDING_SHARE_TTL_SECONDS,
        };
        let result = crate::sharing::retry_pending_shares(
            &mut self.client,
            resolver_transport,
            &params,
            &mut self.rng,
        )
        .await;
        self.signoff(result).await
    }

    // -- Account config --

    /// Fetch the account config record, if it exists.
    pub async fn get_account_config(
        &mut self,
    ) -> Result<Option<crate::records::AccountConfigRecord>, Error> {
        let result = crate::account_config::fetch_account_config(&mut self.client, &self.did).await;
        self.signoff(result).await
    }

    /// Write the account config record (upsert).
    pub async fn set_account_config(
        &mut self,
        config: &crate::records::AccountConfigRecord,
    ) -> Result<String, Error> {
        // Update cached appview URL when config explicitly sets one.
        // Don't overwrite the compile-time default with None.
        if config.appview_url.is_some() {
            self.appview_url = config.appview_url.clone();
        }
        let result = crate::account_config::publish_account_config(&mut self.client, config).await;
        self.signoff(result).await
    }

    // -- Cross-PDS document download --

    /// Download a workspace document as a member (cross-PDS, first-time).
    ///
    /// Uses the transport for unauthenticated public endpoint calls to the
    /// document owner's PDS. Returns the download result plus the keyring
    /// rkey and rotation for local caching.
    pub async fn download_as_workspace_member(
        &mut self,
        document_uri: &str,
    ) -> Result<crate::documents::KeyringDownloadResult, Error> {
        let identity = self.require_identity()?;
        let private_key = identity.private_key_bytes()?;
        crate::documents::download_from_keyring_member(
            self.client.transport(),
            &self.did,
            &private_key,
            document_uri,
        )
        .await
    }

    // -- Identity resolution --

    /// Resolve another user's identity (DID, handle, public key).
    ///
    /// Cross-PDS — uses the transport for unauthenticated lookups.
    pub async fn resolve_identity(
        &self,
        handle_or_did: &str,
    ) -> Result<crate::resolve::ResolvedIdentity, Error> {
        let pds_url = self.client.base_url();
        crate::resolve::resolve_identity(self.client.transport(), pds_url, handle_or_did).await
    }

    /// Publish or update the caller's public key record on the PDS.
    pub async fn publish_public_key(&mut self) -> Result<String, Error> {
        let identity = self.require_identity()?;
        let pubkey = identity.public_key_bytes()?;
        let signing_key = identity.verify_key_bytes()?;
        let now = self.now();
        let result = crate::resolve::publish_public_key(
            &mut self.client,
            &pubkey,
            signing_key.as_ref(),
            &now,
        )
        .await;
        self.signoff(result).await
    }

    // -- Pairing --

    /// Create a pair request (new device side). Returns the record ref + ephemeral keypair.
    pub async fn create_pair_request(
        &mut self,
    ) -> Result<(crate::client::RecordRef, crate::crypto::EphemeralKeypair), Error> {
        let now = self.now();
        let result =
            crate::pairing::create_pair_request(&mut self.client, &now, &mut self.rng).await;
        self.signoff(result).await
    }

    /// List pending pair requests on this account.
    pub async fn list_pair_requests(&mut self) -> Result<Vec<crate::client::RecordEntry>, Error> {
        let result = self
            .client
            .list_records(crate::records::PAIR_REQUEST_COLLECTION, None, None)
            .await;
        let page = self.signoff(result).await?;
        Ok(page.records)
    }

    /// List pair response records on this account.
    pub async fn list_pair_responses(&mut self) -> Result<Vec<crate::client::RecordEntry>, Error> {
        let result = self
            .client
            .list_records(crate::records::PAIR_RESPONSE_COLLECTION, Some(100), None)
            .await;
        let page = self.signoff(result).await?;
        Ok(page.records)
    }

    /// Approve a pair request (existing device side).
    pub async fn approve_pair_request(
        &mut self,
        request_uri: &str,
        ephemeral_public_key: &crate::crypto::X25519PublicKey,
    ) -> Result<(), Error> {
        let identity = self.identity.as_ref().ok_or_else(|| {
            Error::NotFound("no identity — generate keys or pair this device first".into())
        })?;
        let now = self.now();
        let result = crate::pairing::respond_to_pair_request(
            &mut self.client,
            identity,
            request_uri,
            ephemeral_public_key,
            &now,
            &mut self.rng,
        )
        .await;
        self.signoff(result).await
    }

    /// Receive a pair response and derive the identity (new device side).
    pub async fn receive_pair_response(
        &mut self,
        response: &crate::records::PairResponse,
        ephemeral_private_key: &crate::crypto::X25519PrivateKey,
    ) -> Result<Identity, Error> {
        crate::pairing::receive_pair_response(
            &mut self.client,
            &self.did,
            response,
            ephemeral_private_key,
        )
        .await
    }

    /// Clean up pair request and response records after successful pairing.
    pub async fn cleanup_pair_records(
        &mut self,
        request_rkey: &str,
        response_rkey: &str,
    ) -> Result<(), Error> {
        let result =
            crate::pairing::cleanup_pair_records(&mut self.client, request_rkey, response_rkey)
                .await;
        self.signoff(result).await
    }

    // -- Daemon tasks --

    /// Clean up expired pair requests older than `ttl_seconds`.
    pub async fn cleanup_expired_pair_requests(
        &mut self,
        ttl_seconds: i64,
    ) -> Result<crate::pairing::CleanupResult, Error> {
        let now = crate::client::time::unix_now();
        let result =
            crate::pairing::cleanup_expired_pair_requests(&mut self.client, now, ttl_seconds).await;
        self.signoff(result).await
    }

    /// Heal stale grants (re-check and fix grant records).
    pub async fn heal_stale_grants(&mut self) -> Result<crate::sharing::HealResult, Error> {
        let result = crate::sharing::heal_stale_grants(&mut self.client).await;
        self.signoff(result).await
    }

    // -- Purge --

    /// Delete all records in a collection. Returns the number of records deleted.
    pub async fn purge_collection(&mut self, collection: &str) -> Result<usize, Error> {
        let mut count = 0;
        loop {
            let page = self
                .client
                .list_records(collection, Some(100), None)
                .await?;
            if page.records.is_empty() {
                break;
            }
            for entry in &page.records {
                let at_uri = atproto::parse_at_uri(&entry.uri)?;
                self.client
                    .delete_record(&at_uri.collection, &at_uri.rkey)
                    .await?;
                count += 1;
            }
        }
        self.auto_persist_session().await?;
        Ok(count)
    }

    // -- Identity recovery --

    /// Save an identity to storage. Used after pairing or mnemonic recovery.
    pub async fn save_identity(&self, identity: &Identity) -> Result<(), Error> {
        self.storage.save_identity(&identity.did, identity).await
    }

    /// Remove the local account (identity, session, config entry).
    pub async fn remove_account(&self) -> Result<(), Error> {
        self.storage.remove_account(&self.did).await
    }

    /// Get a record from the PDS (low-level read).
    pub async fn get_record(
        &mut self,
        did: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<crate::client::RecordEntry, Error> {
        let result = self.client.get_record(did, collection, rkey).await;
        self.signoff(result).await
    }
}

#[cfg(test)]
#[path = "opake_tests.rs"]
mod tests;
