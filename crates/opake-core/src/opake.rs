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
// Time is injected as a single `fn() -> u64` returning microseconds since
// the Unix epoch. CLI passes a chrono-backed fn, WASM passes one backed by
// `js_sys::Date`. RFC 3339 strings for record timestamp fields are derived
// from the same source via `timestamp::rfc3339_from_micros` — one clock, one
// injection, no drift between CLI and WASM formatting.
//
// Session persistence is automatic: after any XRPC call that triggers a
// token refresh, Opake persists the new session through Storage.

use crate::atproto;
use crate::cabinet::Cabinet;
use crate::client::{Transport, XrpcClient};
use crate::crypto::{ContentKey, CryptoRng, DidMember, OwnedPrivateKeys, PublicKeyBundle, RngCore};
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
    pub(crate) identity: Identity,
    pub(crate) rng: R,
    pub(crate) storage: S,
    /// Injected clock returning microseconds since Unix epoch. RFC 3339
    /// timestamps are derived from this via `timestamp::rfc3339_from_micros`,
    /// so there is a single source of truth for "what time is it".
    pub(crate) now_micros_fn: fn() -> u64,
    /// Host-set runtime override — highest priority. Populated via
    /// `set_indexer_url` at boot (CLI: `OPAKE_INDEXER_URL` env var;
    /// web: `VITE_INDEXER_URL`). Lets devs point at localhost regardless
    /// of what's on PDS.
    pub(crate) runtime_indexer_url: Option<String>,
    /// User-configured indexer URL, mirrored from
    /// `accountConfig.indexerUrl` on PDS. Second priority; falls back to
    /// `DEFAULT_INDEXER_URL` when unset.
    pub(crate) config_indexer_url: Option<String>,
    /// Decoded private-key material, populated once at construction and
    /// reused by every RPC entry point that needs to unwrap keys. Avoids
    /// repeated base64-decode + heap allocation of the 2400-byte ML-KEM-768
    /// decapsulation key.
    pub(crate) cached_private_keys: OwnedPrivateKeys,
}

/// Indexer URL baked into the binary at compile time.
///
/// Set via `OPAKE_INDEXER_URL` env var at build time. Falls back to
/// the production URL if unset. Dev builds pick it up from `.envrc`.
pub const DEFAULT_INDEXER_URL: &str = match option_env!("OPAKE_INDEXER_URL") {
    Some(url) => url,
    None => "https://indexer.opake.app",
};

/// Build an authenticated XrpcClient for an already-logged-in account.
///
/// Loads the PDS URL from Config and the session from Storage. Used by
/// identity-less bootstrap flows (pairing) where `Opake::for_account`
/// would otherwise fail with `Error::IdentityMissing`.
pub async fn authenticated_client<T: Transport, S: Storage>(
    storage: &S,
    did: &str,
    transport: T,
) -> Result<XrpcClient<T>, Error> {
    let config = storage.load_config().await?;
    let account = config
        .accounts
        .get(did)
        .ok_or_else(|| Error::NotFound(format!("no account for {did}")))?;
    let session = storage.load_session(did).await?;
    Ok(XrpcClient::with_session(
        transport,
        account.pds_url.clone(),
        session,
    ))
}

impl<T: Transport, R: CryptoRng + RngCore, S: Storage> Opake<T, R, S> {
    pub fn new(
        client: XrpcClient<T>,
        did: String,
        identity: Identity,
        rng: R,
        storage: S,
        now_micros: fn() -> u64,
    ) -> Result<Self, Error> {
        let cached_private_keys = identity.owned_private_keys()?;
        Ok(Self {
            client,
            did,
            identity,
            rng,
            storage,
            now_micros_fn: now_micros,
            runtime_indexer_url: None,
            config_indexer_url: None,
            cached_private_keys,
        })
    }

    // -- Factory --

    /// Build an Opake for a specific account, reading from Storage.
    ///
    /// Loads config (to resolve DID + PDS URL), session (authentication),
    /// and identity (encryption keys). If the identity exists but lacks
    /// signing keys, migrates it automatically.
    ///
    /// Returns `Error::IdentityMissing` if the account is authenticated
    /// (session present) but has no encryption identity yet — callers
    /// should route the user to recovery (seed phrase) or pairing
    /// (another device) to bootstrap one, then retry.
    ///
    /// Indexer URL resolution (highest priority wins):
    /// 1. Runtime env override — CLI checks `OPAKE_INDEXER_URL` after construction
    /// 2. Account config on PDS — fetched best-effort, user-configurable in settings
    /// 3. `DEFAULT_INDEXER_URL` — baked in at compile time from `OPAKE_INDEXER_URL` env var
    ///
    /// Pass `None` for the default account, or `Some(did)` for a specific one.
    pub async fn for_account(
        storage: S,
        did: Option<&str>,
        transport: T,
        mut rng: R,
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

        let mut identity = storage
            .load_identity(&target_did)
            .await
            .map_err(|_| Error::IdentityMissing)?;
        if identity.ensure_signing_keys(&mut rng) {
            let _ = storage.save_identity(&target_did, &identity).await;
        }

        let client = XrpcClient::with_session(transport, account.pds_url.clone(), session);
        let mut opake = Self::new(client, target_did, identity, rng, storage, now_micros)?;
        // Seed `config_indexer_url` (priority 2 of `resolve_indexer_url`) from
        // the user's PDS accountConfig so the priority chain actually works on
        // cold start. Best-effort — offline, missing record, or auth blips
        // fall through silently to the compile-time default. Runtime overrides
        // installed after construction via `set_indexer_url` still win.
        if let Ok(Some(config)) = opake.get_account_config().await {
            opake.config_indexer_url = config.indexer_url;
        }
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
                let cabinet = Cabinet::from_identity(&self.identity)?;
                Ok(FileContext::Cabinet(cabinet))
            }
        }
    }

    /// Build a cabinet FileContext.
    pub fn cabinet_context(&self) -> Result<FileContext, Error> {
        let cabinet = Cabinet::from_identity(&self.identity)?;
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

    /// Resolve a workspace by name via the indexer member-keyrings index.
    ///
    /// The indexer indexes every keyring from the firehose and serves them
    /// via `/api/keyrings` filtered to ones where the caller is a member —
    /// which includes workspaces the caller owns (they're always a member
    /// of their own). That makes it the single source of truth for
    /// name → URI resolution regardless of who owns the keyring.
    ///
    /// Once a unique URI is picked, [`resolve_workspace_by_uri`] fetches
    /// the canonical record from the owner's PDS and unwraps the group
    /// key there — so the indexer is only trusted for the name → URI map,
    /// not for the group key material.
    ///
    /// Limitation: workspace creation writes to the caller's PDS, which
    /// the indexer indexes with some lag (seconds) through the firehose.
    /// A `workspace ls` immediately after `workspace create` may miss the
    /// new entry until Jetstream delivers the commit.
    pub async fn resolve_workspace(&mut self, name: &str) -> Result<Workspace, Error> {
        let keyrings = self.discover_member_keyrings().await?;
        let private_keys = self.private_keys_from_cache();
        let matches: Vec<String> = keyrings
            .iter()
            .filter(|kr| {
                keyrings::decrypt_indexer_keyring_name(kr, &self.did, &private_keys.bundle())
                    .as_deref()
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
            let private_keys = self.private_keys_from_cache();
            let entry = self
                .client
                .get_record(&self.did, &at_uri.collection, &at_uri.rkey)
                .await?;
            let keyring: crate::records::Keyring = serde_json::from_value(entry.value)?;
            let group_key = Self::unwrap_workspace_key(
                &keyring.members,
                &self.did,
                keyring_uri,
                &private_keys.bundle(),
            )?;
            let name = keyrings::decrypt_keyring_name_from_record(&keyring, &group_key)
                .unwrap_or_default();
            let historical_keys = crate::workspace::derive_historical_keys(
                &keyring,
                &self.did,
                keyring_uri,
                &private_keys.bundle(),
            );
            Ok(Workspace::from_keyring(
                keyring_uri.to_string(),
                name,
                None,
                self.did.clone(),
                group_key,
                keyring.rotation,
                historical_keys,
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
        let private_keys = self.private_keys_from_cache();
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
        let group_key = Self::unwrap_workspace_key(
            &keyring.members,
            &self.did,
            keyring_uri,
            &private_keys.bundle(),
        )?;

        // Decrypt metadata for the workspace name
        let name =
            keyrings::decrypt_keyring_name_from_record(&keyring, &group_key).unwrap_or_default();

        let historical_keys = crate::workspace::derive_historical_keys(
            &keyring,
            &self.did,
            keyring_uri,
            &private_keys.bundle(),
        );

        Ok(Workspace::from_keyring(
            keyring_uri.to_string(),
            name,
            None,
            owner_did.to_string(),
            group_key,
            keyring.rotation,
            historical_keys,
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

    /// The caller's encryption identity.
    ///
    /// Always present — Opake requires an identity at construction time.
    /// Accounts that authenticate but haven't bootstrapped an identity yet
    /// (via recovery or pairing) produce `Error::IdentityMissing` from
    /// `for_account` rather than an `Opake` with a dangling handle.
    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    /// Copy the decoded private-key bytes out of the cache as an owned bundle.
    ///
    /// Stack-copies the X25519 (32 B) and ML-KEM-768 (2400 B) secrets without
    /// any base64 decode or heap allocation. Use this instead of calling
    /// `identity.owned_private_keys()` per RPC — that re-decodes base64 every
    /// time.
    fn private_keys_from_cache(&self) -> OwnedPrivateKeys {
        OwnedPrivateKeys {
            x25519: zeroize::Zeroizing::new(*self.cached_private_keys.x25519),
            ml_kem: zeroize::Zeroizing::new(*self.cached_private_keys.ml_kem),
        }
    }

    /// Current RFC 3339 UTC timestamp (microsecond precision).
    pub fn now(&self) -> String {
        crate::timestamp::rfc3339_from_micros((self.now_micros_fn)())
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
        let identity = &self.identity;
        let pubkey = identity.x25519_public_key_bytes()?;
        let mlkem_pubkey = identity.ml_kem_public_key_bytes()?;
        let now = self.now();
        let rkey = self.generate_tid();
        let result = keyrings::create_keyring(
            &mut self.client,
            &CreateKeyringParams {
                name,
                description,
                owner_did: &self.did,
                owner_x25519_public_key: &pubkey,
                owner_ml_kem_public_key: &mlkem_pubkey,
                rkey: &rkey,
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
    /// Discovers all workspaces via the Indexer (includes both owned and member
    /// workspaces). For each:
    /// - Syncs the tree from Indexer
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
    /// Discovers workspaces via Indexer, syncs each one, returns a result per
    /// workspace. Per-workspace errors are captured (not propagated) so one
    /// failing workspace doesn't block the rest.
    pub async fn sync_owned_workspaces_detailed(
        &mut self,
    ) -> Result<Vec<crate::indexer::daemon::WorkspaceSyncResult>, Error> {
        log::trace!("sync: discovering workspaces for {}", self.did);
        let indexer_keyrings = self.discover_member_keyrings().await?;
        log::trace!("sync: found {} workspaces", indexer_keyrings.len());
        let private_keys = self.private_keys_from_cache();

        let mut results = Vec::with_capacity(indexer_keyrings.len());
        for kr in &indexer_keyrings {
            results.push(self.sync_single_workspace(kr, &private_keys.bundle()).await);
        }

        self.auto_persist_session().await?;
        Ok(results)
    }

    /// Sync a single workspace identified by its keyring URI.
    ///
    /// Fetches all member keyrings from the indexer (same as the full sync),
    /// finds the target, and syncs only that one. Returns `None` if the
    /// keyring URI wasn't found in the member list.
    pub async fn sync_workspace_by_uri(
        &mut self,
        keyring_uri: &str,
    ) -> Result<Option<crate::indexer::daemon::WorkspaceSyncResult>, Error> {
        let indexer_keyrings = self.discover_member_keyrings().await?;
        let target = indexer_keyrings.iter().find(|kr| kr.uri == keyring_uri);
        let Some(kr) = target else { return Ok(None) };

        let private_keys = self.private_keys_from_cache();
        let result = self
            .sync_single_workspace(kr, &private_keys.bundle())
            .await;
        self.auto_persist_session().await?;
        Ok(Some(result))
    }

    /// Sync a single workspace: cleanup + apply proposals.
    ///
    /// Captures all errors into `WorkspaceSyncResult::error` instead of
    /// propagating — the caller can continue with remaining workspaces.
    async fn sync_single_workspace(
        &mut self,
        kr: &crate::indexer::IndexerKeyring,
        private_keys: &crate::crypto::PrivateKeyBundle<'_>,
    ) -> crate::indexer::daemon::WorkspaceSyncResult {
        use crate::indexer::daemon::WorkspaceSyncResult;

        let is_owner = kr.owner_did == self.did;
        log::trace!("sync: processing {} (owner={is_owner})", kr.uri);

        let members: Vec<crate::records::KeyringMember> = kr
            .members
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();

        let group_key = match Self::unwrap_workspace_key(&members, &self.did, &kr.uri, private_keys) {
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

        // Indexer DTO doesn't carry `keyHistory`; the daemon-driven sync
        // path builds the workspace without historical keys. That's fine
        // for proposal cleanup / apply, which doesn't decrypt content
        // older than the current rotation. If a sync caller later needs
        // to decrypt rotation-mismatched documents, fetch the full
        // keyring record and rebuild via `resolve_workspace_by_uri`.
        let workspace = crate::workspace::Workspace::from_keyring(
            kr.uri.clone(),
            String::new(),
            None,
            kr.owner_did.clone(),
            group_key,
            kr.rotation,
            Vec::new(),
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
        proposals: &[crate::indexer::KeyringProposal],
    ) -> Result<usize, Error> {
        use crate::crypto;
        use crate::records::{self, keyring_update};

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
        let private_keys = self.private_keys_from_cache();
        let mut group_key = Self::unwrap_workspace_key(
            &keyring.members,
            &self.did,
            keyring_uri,
            &private_keys.bundle(),
        )?;

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
                    // The proposal carries only the proposer's X25519 hint; we
                    // resolve the new member's full hybrid identity from their
                    // published `app.opake.publicKey/self` record so the wrap
                    // covers both halves of the KEM.
                    let resolved = match self.resolve_identity(did).await {
                        Ok(r) => r,
                        Err(e) => {
                            log::warn!("keyring-sync: cannot resolve identity for {did}: {e}");
                            continue;
                        }
                    };
                    let role = p
                        .role
                        .as_deref()
                        .and_then(|r| r.parse::<records::Role>().ok())
                        .unwrap_or(records::Role::Editor);

                    let recipient_bundle = crypto::PublicKeyBundle {
                        x25519: &resolved.x25519_public_key,
                        ml_kem: &resolved.ml_kem_public_key,
                    };
                    let wrapped = crypto::wrap_key(
                        &group_key,
                        &recipient_bundle,
                        did,
                        &crypto::WrapContext::Keyring { uri: keyring_uri },
                        &mut self.rng,
                    )?;
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

                        // New group key requires each member's current hybrid keypair.
                        let mut remaining_pubkeys = Vec::with_capacity(keyring.members.len());
                        for member in &keyring.members {
                            let resolved = self.resolve_identity(member.did()).await?;
                            remaining_pubkeys.push((
                                member.did().to_string(),
                                resolved.x25519_public_key,
                                resolved.ml_kem_public_key,
                            ));
                        }

                        let did_members: Vec<crypto::DidMember<'_>> = remaining_pubkeys
                            .iter()
                            .map(|(d, x_pk, mlkem_pk)| crypto::DidMember {
                                did: d.as_str(),
                                keys: crypto::PublicKeyBundle {
                                    x25519: x_pk,
                                    ml_kem: mlkem_pk,
                                },
                            })
                            .collect();

                        let (new_key, new_wrapped) =
                            crypto::create_group_key(&did_members, keyring_uri, &mut self.rng)?;

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
        proposals: &[crate::indexer::DocumentProposal],
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
        proposal: &crate::indexer::DocumentProposal,
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
    /// from the caller's PDS so the Indexer drops it from its index.
    ///
    /// Metadata proposals (rename, updateDescription) are skipped — verifying
    /// encrypted content isn't practical here. They're idempotent, so the owner
    /// just skips them on re-processing.
    async fn cleanup_own_applied_keyring_proposals(
        &mut self,
        proposals: &[crate::indexer::KeyringProposal],
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
    /// Resolves the member's hybrid public-key bundle internally from the
    /// DID, mirroring the proposal-application path. Callers (including
    /// the WASM binding) only need to pass the DID — fewer byte arrays
    /// crossing the boundary, one resolution path.
    ///
    /// Owner: applies directly. Non-owner manager: creates a keyringUpdate
    /// proposal carrying the new member's DID. The owner re-resolves and
    /// re-wraps when applying.
    pub async fn add_workspace_member(
        &mut self,
        keyring_uri: &str,
        key: &ContentKey,
        member_did: &str,
        role: Role,
    ) -> Result<MutationOutcome, Error> {
        let owner_did = atproto::parse_at_uri(keyring_uri)?.authority.to_string();
        let is_owner = owner_did == self.did();
        let now = self.now();

        // Resolve the member's hybrid bundle from their published
        // publicKey/self record. This is the same path the proposal
        // application code takes, so owner + non-owner stay symmetric.
        let resolved = self.resolve_identity(member_did).await?;
        let member_public_keys = PublicKeyBundle {
            x25519: &resolved.x25519_public_key,
            ml_kem: &resolved.ml_kem_public_key,
        };

        if is_owner {
            keyrings::add_member(
                &mut self.client,
                &AddMemberParams {
                    keyring_uri,
                    group_key: key,
                    new_member_did: member_did,
                    new_member_public_keys: member_public_keys,
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
                member_public_keys.x25519.to_vec(),
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
    /// The Indexer handles visibility immediately (stops listing the workspace).
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
                remaining_pubkeys.push((identity.x25519_public_key, identity.ml_kem_public_key));
            }

            let remaining_keys: Vec<DidMember<'_>> = remaining_dids
                .iter()
                .enumerate()
                .map(|(i, did)| DidMember {
                    did,
                    keys: PublicKeyBundle {
                        x25519: &remaining_pubkeys[i].0,
                        ml_kem: &remaining_pubkeys[i].1,
                    },
                })
                .collect();

            let private_keys = self.private_keys_from_cache();
            let historical_keys = crate::workspace::derive_historical_keys(
                &keyring,
                &self.did,
                keyring_uri,
                &private_keys.bundle(),
            );
            let workspace = Workspace {
                uri: keyring_uri.to_string(),
                name: String::new(),
                description: None,
                owner_did,
                key: group_key.clone(),
                rotation: keyring.rotation,
                historical_keys,
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
        let private_keys = self.private_keys_from_cache();
        crate::documents::download_from_grant(
            self.client.transport(),
            &private_keys.bundle(),
            grant_uri,
        )
        .await
    }

    /// Resolve an incoming grant's document metadata without downloading the blob.
    ///
    /// Cross-PDS — fetches grant + document records from the owner's PDS,
    /// unwraps the content key, decrypts metadata. Returns
    /// `(filename, metadata, createdAt, modifiedAt)` where the timestamps
    /// come from the document record (not the encrypted blob).
    pub async fn resolve_grant_metadata(
        &self,
        grant_uri: &str,
    ) -> Result<(String, crate::crypto::DocumentMetadata, String, Option<String>), Error> {
        let private_keys = self.private_keys_from_cache();
        crate::documents::resolve_grant_metadata(
            self.client.transport(),
            &private_keys.bundle(),
            grant_uri,
        )
        .await
    }

    /// Unwrap a workspace key from keyring member data (pure crypto, no network).
    pub fn unwrap_workspace_key(
        members: &[crate::records::KeyringMember],
        did: &str,
        keyring_uri: &str,
        private_keys: &crate::crypto::PrivateKeyBundle<'_>,
    ) -> Result<ContentKey, Error> {
        let member = members
            .iter()
            .find(|m| m.did() == did)
            .ok_or_else(|| Error::NotFound(format!("no member entry for DID {did}")))?;
        crate::crypto::unwrap_key(
            &member.wrapped_key,
            private_keys,
            &crate::crypto::WrapContext::Keyring { uri: keyring_uri },
        )
    }

    // -- Indexer helpers --

    /// Set the host-level runtime override for the indexer URL.
    ///
    /// Highest priority in `resolve_indexer_url`. Hosts call this at boot
    /// to inject a deployment-specific URL (CLI reads `OPAKE_INDEXER_URL`;
    /// web reads `VITE_INDEXER_URL`). Runtime overrides win over PDS
    /// account config — dev builds pointing at localhost keep working
    /// even when the account's PDS config points at prod.
    pub fn set_indexer_url(&mut self, url: String) {
        self.runtime_indexer_url = Some(url);
    }

    /// Resolve the indexer URL to use for a request.
    ///
    /// Priority (highest first):
    /// 1. Runtime override — `set_indexer_url()`. Host-level knob populated
    ///    at boot from `OPAKE_INDEXER_URL` (CLI) or `VITE_INDEXER_URL`
    ///    (web). Wins so dev/ops overrides aren't undone by stored config.
    /// 2. PDS accountConfig — `config_indexer_url`, mirrored from the
    ///    user's `app.opake.accountConfig` record. Seeded best-effort by
    ///    `for_account` at boot; kept in sync by `set_account_config`.
    /// 3. Compile-time `DEFAULT_INDEXER_URL` — always present.
    ///
    /// Always returns a valid URL. Plain `String` rather than `Result`
    /// because the const fallback is unreachable-free.
    pub fn resolve_indexer_url(&self) -> String {
        self.runtime_indexer_url
            .clone()
            .or_else(|| self.config_indexer_url.clone())
            .unwrap_or_else(|| DEFAULT_INDEXER_URL.to_string())
    }

    // -- SSE token (for EventSource auth) --

    /// Request a short-lived SSE token from the Indexer.
    ///
    /// The token is passed as a query parameter to the SSE endpoint,
    /// sidestepping EventSource's inability to send custom headers.
    pub async fn request_sse_token(&mut self) -> Result<String, Error> {
        let signing_key = self.require_signing_key()?;
        let url = self.resolve_indexer_url();
        crate::indexer::request_sse_token(self.client.transport(), &url, &self.did, &signing_key)
            .await
    }

    // -- Inbox (incoming grants via Indexer) --

    /// Fetch all incoming grants from the Indexer.
    pub async fn list_inbox(&mut self) -> Result<Vec<crate::indexer::InboxGrant>, Error> {
        let signing_key = self.require_signing_key()?;
        let url = self.resolve_indexer_url();
        crate::indexer::fetch_inbox_all(self.client.transport(), &url, &self.did, &signing_key)
            .await
    }

    /// Fetch workspace documents from the Indexer.
    pub async fn list_workspace_documents(
        &mut self,
        keyring_uri: &str,
    ) -> Result<Vec<crate::indexer::WorkspaceDocument>, Error> {
        let signing_key = self.require_signing_key()?;
        let url = self.resolve_indexer_url();
        crate::indexer::fetch_workspace_documents(
            self.client.transport(),
            &url,
            &self.did,
            &signing_key,
            keyring_uri,
        )
        .await
    }

    /// Fetch all keyrings the user is a member of, with full record data.
    pub async fn discover_member_keyrings(
        &mut self,
    ) -> Result<Vec<crate::indexer::IndexerKeyring>, Error> {
        let signing_key = self.require_signing_key()?;
        let url = self.resolve_indexer_url();
        crate::indexer::fetch_member_keyrings(
            self.client.transport(),
            &url,
            &self.did,
            &signing_key,
        )
        .await
    }

    /// Fetch the Ed25519 signing key for Indexer auth, or error if missing.
    ///
    /// Identity migration runs in `for_account`, so a freshly loaded Opake
    /// always has signing keys. The only way to hit this error is a custom
    /// `Opake::new` caller that supplied a legacy Identity without signing
    /// keys — surface the problem rather than quietly returning empty data.
    fn require_signing_key(&self) -> Result<crate::storage::Ed25519SecretKey, Error> {
        self.identity
            .signing_key_bytes()?
            .ok_or_else(|| Error::Auth("identity is missing Ed25519 signing key".into()))
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
        let private_keys = self.private_keys_from_cache();
        let now = crate::client::time::unix_now();
        let pds_url = self.client.base_url().to_owned();
        let params = crate::sharing::RetryParams {
            caller_pds_url: &pds_url,
            owner_did: &self.did,
            owner_private_keys: private_keys.bundle(),
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
    ///
    /// Mirrors `config.indexer_url` into `self.config_indexer_url` — this is
    /// the source-of-truth field for the PDS-configured URL. Host-level
    /// overrides set via `set_indexer_url` live in a separate field and
    /// continue to win priority in `resolve_indexer_url`, so a dev pointing
    /// `VITE_INDEXER_URL` at localhost isn't disturbed by account-config
    /// writes that happen to include `indexerUrl: None` (e.g. `check_session`
    /// touching `modified_at` on a fresh record).
    pub async fn set_account_config(
        &mut self,
        config: &crate::records::AccountConfigRecord,
    ) -> Result<String, Error> {
        self.config_indexer_url = config.indexer_url.clone();
        let result = crate::account_config::publish_account_config(&mut self.client, config).await;
        self.signoff(result).await
    }

    /// Read-merge-write the account config atomically.
    ///
    /// Fetches the current record (or synthesizes a default stamped with
    /// the current `SCHEMA_VERSION`), applies the given partial updates,
    /// refreshes `modified_at`, and writes back. Concurrent callers
    /// serialize under the `&mut self` borrow — no silent clobbers.
    ///
    /// Cuts the WASM boundary crossings in half vs. read-merge-write from
    /// JS (one call instead of two) and keeps the default-record shape
    /// owned by core.
    pub async fn update_account_config(
        &mut self,
        updates: crate::records::AccountConfigUpdates,
    ) -> Result<crate::records::AccountConfigRecord, Error> {
        let now = self.now();
        let current = self.get_account_config().await?;
        let mut next = current.unwrap_or_else(|| crate::records::AccountConfigRecord::new(&now));

        if let Some(v) = updates.telemetry_enabled {
            next.telemetry_enabled = v;
        }
        if let Some(v) = updates.indexer_url {
            next.indexer_url = v;
        }
        next.modified_at = now;

        self.set_account_config(&next).await?;
        Ok(next)
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
        let private_keys = self.private_keys_from_cache();
        crate::documents::download_from_keyring_member(
            self.client.transport(),
            &self.did,
            &private_keys.bundle(),
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
        let identity = &self.identity;
        let pubkey = identity.x25519_public_key_bytes()?;
        let mlkem_pubkey = identity.ml_kem_public_key_bytes()?;
        let signing_key = identity.verify_key_bytes()?;
        let now = self.now();
        let result = crate::resolve::publish_public_key(
            &mut self.client,
            &pubkey,
            &mlkem_pubkey,
            signing_key.as_ref(),
            &now,
        )
        .await;
        self.signoff(result).await
    }

    // -- Pairing (existing-device side) --
    //
    // The new-device side is identity-less and lives in `crate::pairing`
    // as free functions — see that module for the full flow.

    /// List pending pair requests on this account.
    pub async fn list_pair_requests(&mut self) -> Result<Vec<crate::client::RecordEntry>, Error> {
        let result = self
            .client
            .list_records(crate::records::PAIR_REQUEST_COLLECTION, None, None)
            .await;
        let page = self.signoff(result).await?;
        Ok(page.records)
    }

    /// Approve a pair request by wrapping this device's identity to the
    /// requester's ephemeral hybrid public-key bundle and publishing the
    /// response.
    pub async fn approve_pair_request(
        &mut self,
        request_uri: &str,
        ephemeral_x25519_public_key: &crate::crypto::X25519PublicKey,
        ephemeral_ml_kem_public_key: &crate::crypto::MlKemPublicKey,
    ) -> Result<(), Error> {
        let now = self.now();
        let bundle = crate::crypto::PublicKeyBundle {
            x25519: ephemeral_x25519_public_key,
            ml_kem: ephemeral_ml_kem_public_key,
        };
        let result = crate::pairing::respond_to_pair_request(
            &mut self.client,
            &self.identity,
            request_uri,
            &bundle,
            &now,
            &mut self.rng,
        )
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
