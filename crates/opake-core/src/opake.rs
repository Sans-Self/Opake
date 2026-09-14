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
use crate::crypto::{ContentKey, CryptoRng, OwnedPrivateKeys, PublicKeyBundle, RngCore};
use crate::directories::ChainHeadProvider;
use crate::error::Error;
use crate::indexer::retry::retry_visibility;
use crate::keyrings::{self, CreateKeyringParams, KEYRING_COLLECTION};
use crate::manager::MutationOutcome;
use crate::manager::{FileContext, FileManager, WorkspaceAdmin};
use crate::records::Role;
use crate::resolve::RecipientVerificationNotice;
use crate::storage::{Identity, Storage};
use crate::workspace::{Workspace, WorkspaceId};
use futures_util::future::{select, Either, FutureExt};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Bound unattended repair work so a large workspace cannot monopolize a
/// daemon pass. An explicit user-triggered repair does not use this limit.
const MAX_MEMBER_WRAP_REPAIRS_PER_PASS: usize = 32;
/// Retry a transient indexer visibility gap on a later pass, rather than on
/// every daemon tick for the same observed keyring head.
const MEMBER_WRAP_VISIBILITY_BACKOFF_MICROS: u64 = 20 * 60 * 1_000_000;
const MEMBER_WRAP_HUMAN_DECISION_BACKOFF_MICROS: u64 = 20 * 60 * 1_000_000;
/// Upper wall-clock budget for one unattended repair scan. An individual
/// network operation retains its own bounded visibility retry; this prevents
/// a pass from starting further work once earlier candidates used the budget.
const MEMBER_WRAP_REPAIR_PASS_BUDGET_MICROS: u64 = 2 * 60 * 1_000_000;
/// A slow beneficiary directory must not prevent a manager from revoking a
/// different member. Timed-out beneficiaries remain admitted without a new
/// wrap and retain their historical entry for a later repair.
const MEMBER_REMOVAL_RECIPIENT_RESOLUTION_BUDGET_MICROS: u64 = 10 * 1_000_000;

fn rotate_member_wrap_workspaces<T>(workspaces: &mut [T], cursor: &mut usize) {
    if workspaces.is_empty() {
        return;
    }
    let start = *cursor % workspaces.len();
    workspaces.rotate_left(start);
    *cursor = (start + 1) % workspaces.len();
}

fn member_wrap_deferral_is_active(
    deferrals: &HashMap<String, u64>,
    key: &str,
    now_micros: u64,
) -> bool {
    deferrals
        .get(key)
        .is_some_and(|retry_at| *retry_at > now_micros)
}

fn retain_live_member_wrap_deferrals(
    deferrals: &mut HashMap<String, u64>,
    live_heads: &HashSet<String>,
) {
    deferrals.retain(|member_key, _| {
        live_heads
            .iter()
            .any(|head| member_key.starts_with(&format!("{head}\u{1f}")))
    });
}

async fn within_member_wrap_budget<F>(
    timer: Option<crate::client::identity_operation::IdentitySleepFn>,
    remaining: Duration,
    future: F,
) -> Option<F::Output>
where
    F: std::future::Future,
{
    let Some(timer) = timer else {
        return Some(future.await);
    };
    match select(future.boxed_local(), timer(remaining).boxed_local()).await {
        Either::Left((result, _)) => Some(result),
        Either::Right((_, _)) => None,
    }
}

#[cfg(test)]
mod member_wrap_repair_scheduler_tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn next_pass_starts_at_next_workspace_after_early_failures() {
        let mut cursor = 0;
        let mut first_pass = vec!["stuck", "healthy"];
        rotate_member_wrap_workspaces(&mut first_pass, &mut cursor);
        assert_eq!(first_pass, ["stuck", "healthy"]);

        let mut next_pass = vec!["stuck", "healthy"];
        rotate_member_wrap_workspaces(&mut next_pass, &mut cursor);
        assert_eq!(next_pass, ["healthy", "stuck"]);
    }

    #[test]
    fn human_cooldown_expires_and_allows_recheck() {
        let key = "at://owner/kr/head\u{1f}did:plc:member";
        let mut deferrals = HashMap::new();
        deferrals.insert(key.to_owned(), 1_000);

        assert!(member_wrap_deferral_is_active(&deferrals, key, 999));
        assert!(!member_wrap_deferral_is_active(&deferrals, key, 1_000));
    }

    #[test]
    fn changed_head_invalidates_human_cooldown() {
        let old_key = "at://owner/kr/old\u{1f}did:plc:member";
        let mut deferrals = HashMap::new();
        deferrals.insert(old_key.to_owned(), u64::MAX);
        let live_heads = HashSet::from(["at://owner/kr/new".to_owned()]);

        retain_live_member_wrap_deferrals(&mut deferrals, &live_heads);
        assert!(!member_wrap_deferral_is_active(&deferrals, old_key, 0));
    }

    #[tokio::test]
    async fn pass_timer_defers_a_stuck_discovery_or_candidate() {
        let observed_micros = Rc::new(Cell::new(0_u64));
        let observed_for_timer = Rc::clone(&observed_micros);
        let timer: crate::client::identity_operation::IdentitySleepFn = Rc::new(move |duration| {
            observed_for_timer.set(duration.as_micros() as u64);
            Box::pin(async {})
        });

        let result: Option<()> = within_member_wrap_budget(
            Some(timer),
            Duration::from_micros(123),
            std::future::pending(),
        )
        .await;

        assert_eq!(result, None);
        assert_eq!(observed_micros.get(), 123);
    }
}

/// Pick the head URI of the workspace whose decrypted name is `name`.
///
/// A miss is [`Error::NotFound`] — the visibility-gap class, because the
/// indexer's listing may simply not carry a just-created workspace yet. An
/// ambiguous name is not: two live workspaces answer to it and waiting cannot
/// change that.
fn match_workspace_name(
    workspaces: &[crate::indexer::types::IndexerEnvelope<crate::records::Keyring>],
    name: &str,
    did: &str,
    private_keys: &OwnedPrivateKeys,
) -> Result<String, Error> {
    let matches: Vec<String> = workspaces
        .iter()
        .filter(|ws| {
            keyrings::decrypt_indexer_workspace_name(ws, did, &private_keys.bundle()).as_deref()
                == Some(name)
        })
        // Resolve by head URI — the envelope's URI IS the current canonical
        // keyring record (after any manager supersede).
        .map(|env| env.uri.clone())
        .collect();

    match matches.as_slice() {
        [] => Err(Error::NotFound(format!("no keyring named {name:?}"))),
        [uri] => Ok(uri.clone()),
        uris => Err(Error::AmbiguousName {
            name: name.to_string(),
            count: uris.len(),
            uris: uris.to_vec(),
        }),
    }
}

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
    /// Injected async sleep used to space out retries when a dependent
    /// operation waits for the indexer to catch up with a prior own-write
    /// (see [`crate::indexer::retry`]). Platform-supplied — `tokio::time`
    /// on native, `setTimeout` on the web — via [`Opake::set_sleep_fn`], the
    /// same injection pattern the SSE reconnect loop uses. When unset (e.g.
    /// a bare test context), the visibility-gap retry degrades to a single
    /// attempt rather than fabricating a runtime dependency inside core.
    pub(crate) sleep_fn: Option<crate::indexer::retry::SleepFn>,
    /// Cloneable platform sleep for bounded, independently owned identity
    /// operations. Unlike the indexer retry sleeper, an operation must retain
    /// this after it leaves `Opake`, so the callback uses an `Rc`-backed type.
    pub(crate) identity_sleep_fn: Option<crate::client::identity_operation::IdentitySleepFn>,
    /// Ephemeral scheduling-only cache for missing wraps that need a human
    /// confirmation. It is keyed by the observed head and never authorizes
    /// membership or survives an `Opake` instance.
    member_wrap_human_deferrals: HashMap<String, u64>,
    /// Earliest retry time for a head that was temporarily not visible.
    member_wrap_visibility_backoff: HashMap<String, u64>,
    /// Start each background pass at the next workspace so a repeatedly
    /// failing early workspace cannot consume every bounded slot forever.
    member_wrap_workspace_cursor: usize,
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

/// Outcome of [`Opake::create_workspace`]. The workspace's sidebar entry is
/// not built from this — that arrives via the indexer echo/snapshot, never
/// optimistically. What the caller genuinely needs at creation time is the
/// stable identity (to key the group-key cache and to await the echo) and
/// the group key itself.
pub struct CreatedWorkspace {
    /// The genesis keyring URI (also the workspace's stable identity).
    pub keyring_uri: String,
    /// The unwrapped group key for the new workspace.
    pub key: ContentKey,
}

/// Why a member remained admitted but received no wrap in a removal rotation.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExcludedMemberReason {
    /// A verified account's published record failed its DID-document check.
    VerificationFailed,
    /// An unverified bundle lacks matching recorded approval and can be
    /// repaired only after a manager confirms the freshly resolved bundle.
    ApprovalRequired,
    /// The recipient could not be resolved at all; no override is offered
    /// until a later resolution produces a concrete state.
    ResolutionFailed,
}

/// Fresh, non-secret state used by clients to explain a member's current
/// access. It deliberately derives approval from the live head and current
/// recipient bundle; an approval copied from an older member snapshot must
/// never decide whether a new confirmation is shown.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMemberAccessStatus {
    pub did: String,
    pub has_current_wrap: bool,
    pub verification: MemberVerificationStatus,
    /// Whether the caller is a manager and can presently unwrap the live
    /// group key. Approval can still be recorded without this capability.
    pub can_repair: bool,
}

/// The verification and approval result for one freshly resolved member.
#[derive(Debug, Clone, Copy, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MemberVerificationStatus {
    Verified,
    /// The current unverified bundle exactly matches the approval on the
    /// live head, so a repair does not need another confirmation.
    UnverifiedApproved,
    /// The current unverified bundle needs a manager's explicit approval.
    UnverifiedApprovalRequired,
    /// A DID-document verification failure is a refusal, never an approval
    /// prompt or override opportunity.
    VerificationError,
    /// The recipient could not be resolved well enough to determine whether
    /// an approval applies. This also offers no override until it resolves.
    ResolutionError,
}

/// A member deliberately excluded from a new group-key generation.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcludedMember {
    pub did: String,
    pub reason: ExcludedMemberReason,
}

/// Result of a manager-authored removal rotation. `excluded_members` remain
/// admitted but received no wrap for the new generation, so callers can name
/// the repair that is required without mistaking them for removals.
#[derive(Debug)]
pub struct WorkspaceMemberRemoval {
    pub group_key: ContentKey,
    pub rotation: u64,
    pub excluded_members: Vec<ExcludedMember>,
    /// Fresh verification states observed while producing the new member
    /// wraps. These are notices only; verified replacement/no-history states
    /// never create a second approval requirement.
    pub verification_notices: Vec<RecipientVerificationNotice>,
}

/// Result of a membership write that resolved a recipient bundle at its final
/// write boundary. The notice is intentionally the full verification state so
/// clients can distinguish no history from an unverified bundle.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMemberWriteResult {
    pub verification_notice: RecipientVerificationNotice,
}

/// Aggregate result of an unattended missing-member-wrap repair pass. It
/// contains no group keys or approval commitments; all remaining work is
/// still represented by the live keyring head.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberWrapRepairOutcome {
    pub attempted: usize,
    pub repaired: usize,
    pub awaiting_approval: usize,
    pub verification_failed: usize,
    pub stale: usize,
    pub deferred_human_decision: usize,
    pub deferred_visibility: usize,
    pub deferred_by_budget: usize,
    /// The full-list discovery itself exceeded the pass timer, so no repair
    /// candidates were started this pass.
    pub discovery_deferred: bool,
    /// Final resolver states for wraps actually repaired in this pass.
    pub verification_notices: Vec<RecipientVerificationNotice>,
    pub skipped_not_manager: usize,
    pub skipped_without_current_key: usize,
    pub errors: usize,
}

/// Result of a first-time cross-PDS member download.
///
/// Carries the decrypted bytes plus the document's keyring rotation so the
/// caller can cache the key that actually decrypted this document. That key
/// may be historical when the member has no current wrap.
#[derive(Debug)]
pub struct KeyringDownloadResult {
    pub filename: String,
    pub plaintext: Vec<u8>,
    pub group_key: ContentKey,
    pub keyring_rkey: String,
    pub rotation: u64,
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
            sleep_fn: None,
            identity_sleep_fn: None,
            member_wrap_human_deferrals: HashMap::new(),
            member_wrap_visibility_backoff: HashMap::new(),
            member_wrap_workspace_cursor: 0,
        })
    }

    /// Install the async sleep used by the dependent-operation retry
    /// ([`crate::indexer::retry`]). Front-ends call this at construction with
    /// their platform sleep — `tokio::time::sleep` on the CLI/daemon, a
    /// `setTimeout` promise on the web — mirroring how they inject
    /// `now_micros`. Without it the visibility-gap retry cannot wait and
    /// degrades to a single attempt.
    pub fn set_sleep_fn(&mut self, sleep_fn: crate::indexer::retry::SleepFn) {
        self.sleep_fn = Some(sleep_fn);
    }

    /// Install the platform timer used to bound identity authorization and
    /// cleanup I/O. The operation owns a clone, allowing its cancellation
    /// handle to remain usable while an async completion method is suspended.
    pub fn set_identity_sleep_fn(
        &mut self,
        sleep_fn: crate::client::identity_operation::IdentitySleepFn,
    ) {
        self.identity_sleep_fn = Some(sleep_fn);
    }

    /// Resolve this account's DID document directly and classify the local
    /// verification method. Bootstrap surfaces this result; it never uses PDS
    /// recommended credentials, which cannot report a foreign substitution.
    pub async fn check_own_verification(
        &self,
    ) -> Result<crate::resolve::SelfVerificationState, Error> {
        let key = self
            .identity
            .verify_key_bytes()?
            .ok_or(Error::IdentityMissing)?;
        crate::resolve::check_own_verification(self.client.transport(), &self.did, &key).await
    }

    /// The bootstrap path must not wait indefinitely on the public DID
    /// directory. Platforms that install an identity timer receive an explicit
    /// timeout error which clients surface as an unavailable self-check.
    pub async fn check_own_verification_bounded(
        &self,
    ) -> Result<crate::resolve::SelfVerificationState, Error> {
        let sleep = self.identity_sleep_fn.clone().ok_or_else(|| {
            Error::Auth("identity self-check requires an injected platform timer".into())
        })?;
        crate::client::identity_operation::bounded(&sleep, self.check_own_verification()).await?
    }

    /// Create an opaque, unpersisted operation that will publish this device's
    /// Ed25519 verification key. Construction is synchronous so an interface
    /// can retain its cancellation handle before OAuth discovery begins.
    pub fn new_verification_method_publication(
        &mut self,
        redirect_uri: String,
    ) -> Result<
        (
            crate::client::identity_operation::IdentityOperation<T>,
            crate::client::identity_operation::IdentityOperationCancellation,
        ),
        Error,
    >
    where
        T: Clone,
    {
        let key = self
            .identity
            .verify_key_bytes()?
            .ok_or(Error::IdentityMissing)?;
        self.new_verification_method_operation(
            redirect_uri,
            crate::client::identity_operation::VerificationMethodChange::Publish(key),
        )
    }

    /// Publish the signed public-key record before creating the PLC operation
    /// that would make that signature mandatory for counterparties. Callers
    /// should retain the returned cancellation handle before starting OAuth.
    pub async fn prepare_verification_method_publication(
        &mut self,
        redirect_uri: String,
    ) -> Result<
        (
            crate::client::identity_operation::IdentityOperation<T>,
            crate::client::identity_operation::IdentityOperationCancellation,
        ),
        Error,
    >
    where
        T: Clone,
    {
        self.publish_public_key().await?;
        self.new_verification_method_publication(redirect_uri)
    }

    /// Create an opaque, unpersisted operation that will remove `#opake`.
    pub fn new_verification_method_removal(
        &mut self,
        redirect_uri: String,
    ) -> Result<
        (
            crate::client::identity_operation::IdentityOperation<T>,
            crate::client::identity_operation::IdentityOperationCancellation,
        ),
        Error,
    >
    where
        T: Clone,
    {
        self.new_verification_method_operation(
            redirect_uri,
            crate::client::identity_operation::VerificationMethodChange::Remove,
        )
    }

    fn new_verification_method_operation(
        &mut self,
        redirect_uri: String,
        change: crate::client::identity_operation::VerificationMethodChange,
    ) -> Result<
        (
            crate::client::identity_operation::IdentityOperation<T>,
            crate::client::identity_operation::IdentityOperationCancellation,
        ),
        Error,
    >
    where
        T: Clone,
    {
        let sleep = self.identity_sleep_fn.clone().ok_or_else(|| {
            Error::Auth("identity operations require an injected platform timer".into())
        })?;
        Ok(crate::client::identity_operation::IdentityOperation::new(
            self.client.transport().clone(),
            crate::client::identity_operation::IdentityOperationConfig {
                pds_url: self.client.base_url().to_owned(),
                did: self.did.clone(),
                redirect_uri,
                change,
                now_micros: self.now_micros_fn,
                sleep,
            },
            &mut self.rng,
        ))
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
    /// Name resolution is a dependent operation: workspace creation writes the
    /// genesis keyring to the caller's PDS, and the indexer only lists it once
    /// the firehose delivers that commit. A create-then-mutate — the CLI's
    /// `workspace create X` followed immediately by `workspace add-member X` —
    /// would otherwise fail on the first listing that does not carry X yet. So
    /// the listing runs inside the bounded visibility window (see
    /// [`crate::indexer::retry`]): an unmatched name is absorbed on the backoff
    /// schedule, and only the window's exhaustion surfaces an error naming the
    /// workspace that was awaited.
    ///
    /// Trade-off: a genuinely mistyped name burns the full window before
    /// erroring. Workspace names live in encrypted metadata the indexer never
    /// sees, so "not listed yet" and "never existed" are the same answer at the
    /// wire — no signal exists to tell them apart and fail the typo fast.
    pub async fn resolve_workspace(&mut self, name: &str) -> Result<Workspace, Error> {
        let uri = self.resolve_workspace_uri(name).await?;
        self.resolve_workspace_by_uri(&uri).await
    }

    /// The retried half of [`resolve_workspace`]: list the caller's workspaces
    /// and pick the one whose decrypted name matches, absorbing the visibility
    /// gap while the listing does not carry it yet.
    ///
    /// Borrows are split by field rather than routed through `&mut self`
    /// methods, which is what lets the listing closure and the injected sleeper
    /// coexist inside [`retry_visibility`].
    async fn resolve_workspace_uri(&mut self, name: &str) -> Result<String, Error> {
        let signing_key = self.require_signing_key()?;
        let indexer_url = self.resolve_indexer_url();
        let private_keys = self.private_keys_from_cache();
        let now_micros = self.now_micros_fn;
        let did = self.did.as_str();
        let transport = self.client.transport();

        let list_and_match = || async {
            let workspaces =
                crate::indexer::fetch_member_workspaces(transport, &indexer_url, did, &signing_key)
                    .await?;
            match_workspace_name(&workspaces, name, did, &private_keys)
        };

        // Without an injected sleeper (a bare test context) we cannot wait, so
        // the single-shot answer stands — the same fallback
        // `fetch_keyring_chain_head` makes.
        match self.sleep_fn.as_mut() {
            Some(sleep) => {
                retry_visibility(
                    &format!("resolving workspace {name:?}"),
                    now_micros,
                    sleep,
                    list_and_match,
                )
                .await
            }
            None => list_and_match().await,
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
            // `keyring_uri` is the head the caller fetched; the stable
            // workspace identity (genesis URI) is what wraps anchor to and
            // what every downstream consumer — chain lookups, doc refs,
            // cache keys — must key on.
            let workspace_id = keyring.lineage_anchor(keyring_uri).to_string();
            let group_key = Self::try_unwrap_workspace_key(
                &keyring.members,
                &self.did,
                &workspace_id,
                &private_keys.bundle(),
                keyring.opake_version,
            )?;
            let historical_keys = crate::workspace::derive_historical_keys(
                &keyring,
                &self.did,
                &workspace_id,
                &private_keys.bundle(),
            );
            let name = group_key
                .as_ref()
                .and_then(|key| {
                    keyrings::decrypt_keyring_name_from_record(&keyring, key, &workspace_id)
                })
                .unwrap_or_default();
            // spec: workspace-identity § Identity adoption verifies by derivation
            if !crate::workspace::verify_workspace_identity(
                &keyring,
                &workspace_id,
                group_key.as_ref(),
                &historical_keys,
            ) {
                return Err(Error::WorkspaceIdentityMismatch {
                    anchor: workspace_id,
                });
            }
            let manager_dids = crate::workspace::manager_dids_from_keyring(&keyring);
            Ok(Workspace::from_keyring(
                workspace_id,
                name,
                None,
                self.did.clone(),
                group_key,
                keyring.rotation,
                historical_keys,
                manager_dids,
            ))
        } else {
            // Foreign keyring — resolve via public PDS endpoint
            self.resolve_foreign_workspace(keyring_uri).await
        }
    }

    /// Fetch a keyring's member list by URI.
    ///
    /// Resolves the keyring authority's PDS and reads the record over the
    /// public endpoint, so one path serves owners and cross-PDS members
    /// alike — routing a foreign `getRecord` through the caller's own PDS
    /// would depend on appview pipethrough, which Opake does not require.
    pub async fn workspace_members(
        &self,
        keyring_uri: &str,
    ) -> Result<Vec<crate::records::KeyringMember>, Error> {
        let at_uri = crate::atproto::parse_at_uri(keyring_uri)?;
        let did_doc =
            crate::client::resolve_did_document(self.client.transport(), &at_uri.authority).await?;
        let pds_url = crate::client::pds_from_did_document(&did_doc)?;
        let entry = crate::client::get_record_public(
            self.client.transport(),
            &pds_url,
            &at_uri.authority,
            &at_uri.collection,
            &at_uri.rkey,
        )
        .await?;
        let keyring: crate::records::Keyring = serde_json::from_value(entry.value)?;
        Ok(keyring.members)
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
        // Stable (genesis) identity — the wrap anchor and the id every
        // downstream consumer keys on. See resolve_workspace_by_uri.
        let workspace_id = keyring.lineage_anchor(keyring_uri).to_string();
        let group_key = Self::try_unwrap_workspace_key(
            &keyring.members,
            &self.did,
            &workspace_id,
            &private_keys.bundle(),
            keyring.opake_version,
        )?;

        let historical_keys = crate::workspace::derive_historical_keys(
            &keyring,
            &self.did,
            &workspace_id,
            &private_keys.bundle(),
        );

        // Current metadata is encrypted under the current key. A
        // historical-only member still adopts the workspace, with its name
        // populated once a current wrap is repaired.
        let name = group_key
            .as_ref()
            .and_then(|key| {
                keyrings::decrypt_keyring_name_from_record(&keyring, key, &workspace_id)
            })
            .unwrap_or_default();

        // spec: workspace-identity § Identity adoption verifies by derivation
        if !crate::workspace::verify_workspace_identity(
            &keyring,
            &workspace_id,
            group_key.as_ref(),
            &historical_keys,
        ) {
            return Err(Error::WorkspaceIdentityMismatch {
                anchor: workspace_id,
            });
        }

        let manager_dids = crate::workspace::manager_dids_from_keyring(&keyring);
        Ok(Workspace::from_keyring(
            workspace_id,
            name,
            None,
            owner_did.to_string(),
            group_key,
            keyring.rotation,
            historical_keys,
            manager_dids,
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
        rkey: Option<&str>,
        record: &impl serde::Serialize,
    ) -> Result<crate::client::RecordRef, Error> {
        let result = self.client.create_record(collection, rkey, record).await;
        self.signoff(result).await
    }

    // -- Workspace management (keyring operations) --

    /// Create a new workspace. Returns the created keyring URI (the stable
    /// workspace identity) and the group key. The sidebar entry is delivered
    /// by the indexer echo/snapshot, not built optimistically from this.
    pub async fn create_workspace(
        &mut self,
        name: &str,
        description: Option<&str>,
    ) -> Result<CreatedWorkspace, Error> {
        let identity = &self.identity;
        let pubkey = identity.x25519_public_key_bytes()?;
        let mlkem_pubkey = identity.ml_kem_public_key_bytes()?;
        let now = self.now();
        let (keyring_uri, key) = keyrings::create_keyring(
            &mut self.client,
            &CreateKeyringParams {
                name,
                description,
                owner_did: &self.did,
                owner_x25519_public_key: &pubkey,
                owner_ml_kem_public_key: &mlkem_pubkey,
                created_at: &now,
            },
            &mut self.rng,
        )
        .await?;
        self.auto_persist_session().await?;
        Ok(CreatedWorkspace { keyring_uri, key })
    }

    /// Sync all workspaces: load the chain head tree for each.
    ///
    /// Discovers all workspaces via the Indexer (includes both owned and member
    /// workspaces). Loads the tree for each so SSE consumers have a baseline to
    /// patch from. Returns the number of workspaces processed.
    pub async fn sync_owned_workspaces(&mut self) -> Result<usize, Error> {
        let results = self.sync_owned_workspaces_detailed().await?;
        Ok(results.len())
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
        let workspaces = self.discover_member_workspaces().await?;
        log::trace!("sync: found {} workspaces", workspaces.len());
        let private_keys = self.private_keys_from_cache();

        let mut results = Vec::with_capacity(workspaces.len());
        for ws in &workspaces {
            results.push(self.sync_single_workspace(ws, &private_keys.bundle()).await);
        }

        self.auto_persist_session().await?;
        Ok(results)
    }

    /// Sync a single workspace identified by its workspace ID (genesis
    /// keyring URI).
    ///
    /// Fetches all member workspaces from the indexer (same as the
    /// full sync), finds the target, and syncs only that one. Returns
    /// `None` if the workspace wasn't found in the member list.
    pub async fn sync_workspace_by_uri(
        &mut self,
        workspace_id: &WorkspaceId,
    ) -> Result<Option<crate::indexer::daemon::WorkspaceSyncResult>, Error> {
        let workspaces = self.discover_member_workspaces().await?;
        let target = workspaces
            .iter()
            .find(|env| env.workspace_id() == *workspace_id);
        let Some(ws) = target else { return Ok(None) };

        let private_keys = self.private_keys_from_cache();
        let result = self.sync_single_workspace(ws, &private_keys.bundle()).await;

        self.auto_persist_session().await?;
        Ok(Some(result))
    }

    /// Sync a single workspace: load the tree so SSE consumers can patch it.
    ///
    /// Pre-federation this method also applied proposals from the workspace
    /// owner's perspective and cleaned up the caller's already-applied
    /// proposals. The federation rewrite replaces both with chain-aware
    /// curatorial supersedes — every chain participant writes directly to
    /// their own PDS, so there's nothing to apply on someone else's behalf.
    async fn sync_single_workspace(
        &mut self,
        envelope: &crate::indexer::types::IndexerEnvelope<crate::records::Keyring>,
        private_keys: &crate::crypto::PrivateKeyBundle<'_>,
    ) -> crate::indexer::daemon::WorkspaceSyncResult {
        use crate::indexer::daemon::WorkspaceSyncResult;

        let head_uri = envelope.uri.clone();
        let workspace_id = envelope
            .record
            .lineage
            .clone()
            .unwrap_or_else(|| head_uri.clone());

        // The head's authoring DID lives in the at-uri authority position.
        let head_pds_did = head_uri
            .strip_prefix("at://")
            .and_then(|rest| rest.split('/').next())
            .unwrap_or("")
            .to_string();
        let is_owner = head_pds_did == self.did;
        log::trace!("sync: processing {head_uri} (owner={is_owner})");

        // Member wraps anchor to the stable (genesis) workspace_id, not the
        // head — the same context resolve_workspace_by_uri uses.
        let group_key = match Self::try_unwrap_workspace_key(
            &envelope.record.members,
            &self.did,
            &workspace_id,
            private_keys,
            envelope.record.opake_version,
        ) {
            Ok(k) => k,
            Err(e) => {
                return WorkspaceSyncResult {
                    keyring_uri: head_uri,
                    is_owner,
                    error: Some(format!("key unwrap: {e}")),
                };
            }
        };

        let historical_keys = crate::workspace::derive_historical_keys(
            &envelope.record,
            &self.did,
            &head_uri,
            private_keys,
        );

        // The CLI has no keepers; this sync loop is its sole workspace-identity
        // adoption surface, so the derivation check must run here too — an
        // unverified adoption on this path would leave the forged-keyring
        // vector open exactly where the keeper closes it.
        // spec: workspace-identity § Identity adoption verifies by derivation
        if !crate::workspace::verify_workspace_identity(
            &envelope.record,
            &workspace_id,
            group_key.as_ref(),
            &historical_keys,
        ) {
            return WorkspaceSyncResult {
                keyring_uri: head_uri,
                is_owner,
                error: Some(format!(
                    "workspace identity could not be verified for {workspace_id}"
                )),
            };
        }

        let manager_dids = crate::workspace::manager_dids_from_keyring(&envelope.record);
        let workspace = crate::workspace::Workspace::from_keyring(
            workspace_id,
            String::new(),
            None,
            head_pds_did,
            group_key,
            envelope.record.rotation,
            historical_keys,
            manager_dids,
        );
        let ctx = crate::manager::FileContext::Workspace(workspace);
        let mut mgr = self.file_manager(&ctx);
        let error = match mgr.load_tree().await {
            Ok(_) => None,
            Err(e) => Some(format!("tree sync: {e}")),
        };

        WorkspaceSyncResult {
            keyring_uri: head_uri,
            is_owner,
            error,
        }
    }

    /// Fetch the current keyring chain head record for a workspace.
    ///
    /// Returns `(head_uri, Keyring)`. Used by every keyring-supersede
    /// path — they all need to know what URI to point `supersedes` at,
    /// and they all need the prior record to mutate.
    ///
    /// The head record may live on any member's PDS; the lookup goes
    /// through the indexer's `chain-head` endpoint, then a cross-PDS
    /// `fetch_chain_node` to retrieve the record itself.
    ///
    /// The indexer is outside the TCB — it can lie about which URI is the
    /// current head. We mitigate with two checks:
    ///
    /// 1. **Chain integrity** — walk back from the indexer's claimed head
    ///    to genesis and verify the genesis URI equals the requested
    ///    `workspace_id`. Closes "indexer points at a head from a
    ///    different workspace's chain."
    /// 2. **Chain authority** — verify every supersede in the chain was
    ///    authored by a manager of the prior keyring. Closes "indexer
    ///    accepted a non-manager's supersede" (e.g., a removed manager
    ///    or a malicious member). Without this, a corrupted indexer
    ///    could let a non-manager rewrite the workspace's authority.
    ///
    /// PDS-signed records mean the indexer can't forge content, only
    /// mislabel or accept invalid writes; these two checks turn both
    /// failure modes into hard rejects rather than silent corruption.
    ///
    /// Note: this does *not* close staleness — a compromised indexer can
    /// still point at an older-but-real head and hide a newer supersede.
    /// Closing staleness requires out-of-band signals (polling each
    /// member's `listRecords`), which is accepted as a distributed-systems
    /// posture, not a security failure.
    ///
    /// This is the resolution boundary the consistency contract names: the
    /// input to a create-then-mutate operation depends on the indexer having
    /// consumed a prior own-write. A workspace creator's genesis keyring, for
    /// instance, is not queryable the instant the PDS accepts it — the
    /// chain-head endpoint answers `workspace_not_indexed` until the firehose
    /// delivers the commit. So this wraps the single-shot resolution in a
    /// bounded retry: that transient signal (and an absent head) is absorbed on
    /// a backoff schedule, and only the window's exhaustion surfaces an error —
    /// a [`Error::VisibilityTimeout`] naming the workspace, distinct from the
    /// definitive [`Error::NotWorkspaceMember`] denial, which is never retried
    /// (see [`crate::indexer::retry`]).
    async fn fetch_keyring_chain_head(
        &mut self,
        workspace_id: &WorkspaceId,
    ) -> Result<(String, String, crate::records::Keyring), Error> {
        use crate::indexer::retry::{is_visibility_gap, VisibilityRetry};

        let start = (self.now_micros_fn)();
        let mut schedule = VisibilityRetry::new();
        loop {
            match self.fetch_keyring_chain_head_once(workspace_id).await {
                Ok(head) => return Ok(head),
                Err(error) if is_visibility_gap(&error) => {
                    let elapsed_ms = (self.now_micros_fn)().saturating_sub(start) / 1_000;
                    // Without an injected sleeper (a bare test context) we
                    // cannot wait, so surface the gap error rather than spin.
                    let Some(sleep) = self.sleep_fn.as_mut() else {
                        return Err(error);
                    };
                    match schedule.next_delay(elapsed_ms) {
                        Some(delay) => sleep(delay).await,
                        None => {
                            return Err(Error::VisibilityTimeout {
                                operation: format!(
                                    "resolving keyring chain head for {workspace_id}"
                                ),
                                waited_ms: elapsed_ms,
                            })
                        }
                    }
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Single-shot keyring chain-head resolution — one indexer round-trip,
    /// chain walk, and authority verification. The retrying wrapper is
    /// [`fetch_keyring_chain_head`]; call this directly only where retry is
    /// explicitly unwanted.
    async fn fetch_keyring_chain_head_once(
        &mut self,
        workspace_id: &WorkspaceId,
    ) -> Result<(String, String, crate::records::Keyring), Error> {
        let url = self.resolve_indexer_url();
        let signing_key = self.require_signing_key()?;
        let provider = crate::indexer::IndexerChainHeadProvider {
            transport: self.client.transport(),
            indexer_url: &url,
            did: &self.did,
            signing_key: &signing_key,
        };
        let heads = provider.workspace_chain_heads(workspace_id).await?;
        let head = heads.keyring.ok_or_else(|| {
            Error::NotFound(format!("workspace {workspace_id} has no indexed keyring"))
        })?;

        // Walk the chain back from the indexer's claimed head to genesis
        // and verify the genesis URI matches `workspace_id`. The returned
        // chain is head→genesis ordered. `chain[0]` is the head record;
        // `chain.last()` is the genesis (validated above).
        let chain = crate::directories::verify_and_walk_chain::<crate::records::Keyring>(
            self.client.transport(),
            &head.uri,
            workspace_id.as_str(),
        )
        .await?;

        // Verify the authorization trail: every supersede must have been
        // authored by a manager of the prior keyring. This runs on the
        // already-fetched chain — no extra network cost.
        crate::directories::verify_keyring_chain_authority(&chain)?;

        // Extract the head — the first node of the verified chain. Safe
        // unwrap: verify_and_walk_chain returns Ok only when the chain
        // is non-empty.
        let head_node = chain.into_iter().next().ok_or_else(|| {
            Error::InvalidRecord("verified chain returned empty result".to_owned())
        })?;
        crate::records::check_version(head_node.record.opake_version)?;
        Ok((head_node.uri, head_node.cid, head_node.record))
    }

    /// Fast local pre-write manager check. Not a security boundary: the
    /// authoritative authorization trail is verified on read by
    /// `verify_keyring_chain_authority`, which walks the keyring chain and
    /// confirms every supersede was authored by a manager of the prior
    /// keyring. This check just produces a clear error before attempting a
    /// write the chain verification would later reject anyway.
    fn require_manager(&self, keyring: &crate::records::Keyring) -> Result<(), Error> {
        let is_manager = keyring
            .members
            .iter()
            .any(|m| m.did() == self.did && matches!(m.role, Role::Manager));
        if is_manager {
            Ok(())
        } else {
            Err(Error::Auth(format!(
                "{} is not a manager of this workspace",
                self.did
            )))
        }
    }

    /// Write a keyring supersede record on caller's PDS, then auto-persist.
    ///
    /// The supersede's `workspace_id` field is filled from the caller-
    /// provided value (the stable genesis URI), not the prior record's
    /// `workspace_id` — both are the same in steady-state but explicit
    /// avoids relying on the prior record having it populated.
    async fn write_keyring_supersede(
        &mut self,
        workspace_id: &WorkspaceId,
        prior_uri: String,
        prior_cid: String,
        mut record: crate::records::Keyring,
    ) -> Result<MutationOutcome, Error> {
        let now = self.now();
        record.supersedes = Some(prior_uri);
        // spec: lineage § Supersede references carry a content pin
        record.supersedes_cid = Some(prior_cid);
        record.lineage = Some(workspace_id.as_str().to_owned());
        record.created_at = now.clone();
        record.modified_at = Some(now);

        self.client
            .create_record(KEYRING_COLLECTION, None, &record)
            .await?;
        self.auto_persist_session().await?;
        Ok(MutationOutcome::Applied)
    }

    /// Inspect the confirmation token required to wrap a workspace key to an
    /// unverified recipient. The caller presents this token to the user, then
    /// passes it to [`Self::add_workspace_member`]; that mutation resolves
    /// again and rejects a token bound to any replaced bundle.
    pub async fn workspace_member_approval_challenge(
        &mut self,
        workspace_id: &WorkspaceId,
        member_did: &str,
    ) -> Result<Option<[u8; 32]>, Error> {
        let (_, _, head) = self.fetch_keyring_chain_head(workspace_id).await?;
        let resolved = self.resolve_identity(member_did).await?;
        match resolved.verification {
            crate::resolve::VerificationState::Verified { .. } => Ok(None),
            crate::resolve::VerificationState::Unverified => {
                Ok(Some(crate::crypto::unverified_key_approval(
                    head.opake_version,
                    workspace_id.as_str(),
                    &resolved.did,
                    &crate::crypto::EncryptionKeyFields {
                        x25519_public_key: &resolved.x25519_public_key,
                        x25519_algo: &resolved.x25519_algo,
                        ml_kem_public_key: &resolved.ml_kem_public_key,
                        ml_kem_algo: &resolved.ml_kem_algo,
                    },
                )))
            }
        }
    }

    /// Inspect an admitted member against the live keyring head. This is an
    /// inspection convenience for clients; every mutation still resolves and
    /// compares the recipient again immediately before it writes.
    pub async fn workspace_member_access_status(
        &mut self,
        workspace_id: &WorkspaceId,
        member_did: &str,
    ) -> Result<WorkspaceMemberAccessStatus, Error> {
        let (_, _, head) = self.fetch_keyring_chain_head(workspace_id).await?;
        let member = head
            .members
            .iter()
            .find(|member| member.did() == member_did)
            .ok_or_else(|| Error::NotFound(format!("no member entry for DID {member_did}")))?;
        let has_current_wrap = member.wrapped_key.is_some();
        let recorded_approval = member.unverified_key_approval.clone();

        let is_manager = head
            .members
            .iter()
            .any(|member| member.did() == self.did && matches!(member.role, Role::Manager));
        let private_keys = self.private_keys_from_cache();
        let can_repair = is_manager
            && Self::try_unwrap_workspace_key(
                &head.members,
                &self.did,
                workspace_id.as_str(),
                &private_keys.bundle(),
                head.opake_version,
            )
            .is_ok_and(|key| key.is_some());

        let verification = match self.resolve_identity(member_did).await {
            Ok(resolved) => match resolved.verification {
                crate::resolve::VerificationState::Verified { .. } => {
                    MemberVerificationStatus::Verified
                }
                crate::resolve::VerificationState::Unverified => {
                    let expected = crate::crypto::unverified_key_approval(
                        head.opake_version,
                        workspace_id.as_str(),
                        member_did,
                        &crate::crypto::EncryptionKeyFields {
                            x25519_public_key: &resolved.x25519_public_key,
                            x25519_algo: &resolved.x25519_algo,
                            ml_kem_public_key: &resolved.ml_kem_public_key,
                            ml_kem_algo: &resolved.ml_kem_algo,
                        },
                    );
                    let approved = recorded_approval
                        .as_ref()
                        .and_then(|approval| approval.decode().ok())
                        .is_some_and(|approval| approval.as_slice() == expected);
                    if approved {
                        MemberVerificationStatus::UnverifiedApproved
                    } else {
                        MemberVerificationStatus::UnverifiedApprovalRequired
                    }
                }
            },
            Err(Error::VerificationFailed(_)) => MemberVerificationStatus::VerificationError,
            Err(_) => MemberVerificationStatus::ResolutionError,
        };

        Ok(WorkspaceMemberAccessStatus {
            did: member_did.to_owned(),
            has_current_wrap,
            verification,
            can_repair,
        })
    }

    /// Add a member to a workspace via curatorial keyring supersede.
    ///
    /// Federation model: any manager can author. The caller's PDS receives
    /// a new keyring record whose `supersedes` points at the indexer-
    /// reported chain head. Authority is validated:
    ///
    /// * Client-side, defensively — early error if the caller's DID isn't
    ///   in the head's manager list.
    /// * Server-side, authoritatively — the indexer's keyring-supersede
    ///   handler checks manager role at the prior record's snapshot.
    ///
    /// `workspace_id` is the genesis keyring URI — stable across the
    /// chain. The new member's wrapped key is bound to it (rather than
    /// the current head URI) so wraps survive future supersedes.
    pub async fn add_workspace_member(
        &mut self,
        workspace_id: &WorkspaceId,
        key: &ContentKey,
        historical_keys: &[crate::workspace::HistoricalKey],
        member_did: &str,
        role: Role,
        confirmed_unverified_keys: Option<[u8; 32]>,
    ) -> Result<WorkspaceMemberWriteResult, Error> {
        let (prior_uri, prior_cid, prior) = self.fetch_keyring_chain_head(workspace_id).await?;
        self.require_manager(&prior)?;
        // The caller's Workspace can lag the live head. Never label an old
        // group key as this head's current generation: re-unwrap our current
        // head wrap and require it to match before authoring any new wraps.
        let private_keys = self.private_keys_from_cache();
        let head_key = Self::unwrap_workspace_key(
            &prior.members,
            &self.did,
            workspace_id.as_str(),
            &private_keys.bundle(),
            prior.opake_version,
        )?;
        if head_key.0 != key.0 {
            return Err(Error::CurrentGroupKeyUnavailable {
                workspace_id: workspace_id.as_str().to_owned(),
            });
        }

        let resolved = self.resolve_identity(member_did).await?;
        let member_did = resolved.did.as_str();
        if prior.members.iter().any(|m| m.did() == member_did) {
            return Err(Error::InvalidRecord(format!(
                "{member_did} is already a member of this workspace"
            )));
        }
        let approval = match &resolved.verification {
            crate::resolve::VerificationState::Verified { .. } => None,
            crate::resolve::VerificationState::Unverified => {
                let expected = crate::crypto::unverified_key_approval(
                    prior.opake_version,
                    workspace_id.as_str(),
                    member_did,
                    &crate::crypto::EncryptionKeyFields {
                        x25519_public_key: &resolved.x25519_public_key,
                        x25519_algo: &resolved.x25519_algo,
                        ml_kem_public_key: &resolved.ml_kem_public_key,
                        ml_kem_algo: &resolved.ml_kem_algo,
                    },
                );
                if confirmed_unverified_keys != Some(expected) {
                    return Err(Error::UnverifiedKeyApprovalRequired {
                        did: member_did.to_owned(),
                    });
                }
                Some(crate::records::AtBytes::from_raw(&expected))
            }
        };
        let member_public_keys = PublicKeyBundle {
            x25519: &resolved.x25519_public_key,
            ml_kem: &resolved.ml_kem_public_key,
        };

        // Wraps for this workspace are anchored to its stable (genesis) URI so
        // they survive future supersedes.
        let wrap_ctx = crate::crypto::WrapContext::Keyring {
            uri: workspace_id.as_str(),
        };

        let wrapped = crate::crypto::wrap_key(
            key,
            &member_public_keys,
            member_did,
            &wrap_ctx,
            &mut self.rng,
        )?;

        let mut new_record = prior;
        let mut member = crate::records::KeyringMember::with_wrap(wrapped, role.clone());
        member.unverified_key_approval = approval;
        new_record.members.push(member);

        // Admission grants the full history, not just the current key: for
        // every retained rotation the admitting manager can still unwrap,
        // wrap that key for the joiner so documents written under prior
        // rotations remain readable to them. A rotation the manager lacks is
        // simply not extended — you cannot grant a key you do not hold.
        // spec:key-rotation § New members can read the full history they are admitted to
        for entry in &mut new_record.key_history {
            if entry.members.iter().any(|m| m.did() == member_did) {
                continue;
            }
            let Some(historical) = historical_keys
                .iter()
                .find(|h| h.rotation == entry.rotation)
            else {
                continue;
            };
            let wrapped_historical = crate::crypto::wrap_key(
                &historical.key,
                &member_public_keys,
                member_did,
                &wrap_ctx,
                &mut self.rng,
            )?;
            entry.members.push(crate::records::KeyringMember::with_wrap(
                wrapped_historical,
                role.clone(),
            ));
        }

        self.write_keyring_supersede(workspace_id, prior_uri, prior_cid, new_record)
            .await?;
        Ok(WorkspaceMemberWriteResult {
            verification_notice: RecipientVerificationNotice {
                did: member_did.to_owned(),
                verification: resolved.verification,
            },
        })
    }

    /// Fill a missing *current* wrap for an admitted member. Repair is a
    /// manager-authored same-rotation supersede: it preserves every other
    /// member entry and all history, and it re-resolves immediately before
    /// writing so stale work cannot re-add a removed member or approve a
    /// replacement bundle.
    pub async fn repair_workspace_member_wrap(
        &mut self,
        workspace_id: &WorkspaceId,
        key: &ContentKey,
        member_did: &str,
        confirmed_unverified_keys: Option<[u8; 32]>,
    ) -> Result<WorkspaceMemberWriteResult, Error> {
        let (prior_uri, prior_cid, prior) = self.fetch_keyring_chain_head(workspace_id).await?;
        self.require_manager(&prior)?;
        let private_keys = self.private_keys_from_cache();
        let head_key = Self::unwrap_workspace_key(
            &prior.members,
            &self.did,
            workspace_id.as_str(),
            &private_keys.bundle(),
            prior.opake_version,
        )?;
        if head_key.0 != key.0 {
            return Err(Error::CurrentGroupKeyUnavailable {
                workspace_id: workspace_id.as_str().to_owned(),
            });
        }

        let position = prior
            .members
            .iter()
            .position(|member| member.did() == member_did)
            .ok_or_else(|| Error::NotFound(format!("no member entry for DID {member_did}")))?;
        if prior.members[position].wrapped_key.is_some() {
            return Err(Error::InvalidRecord(format!(
                "{member_did} already has a current group-key wrap"
            )));
        }

        let resolved = self.resolve_identity(member_did).await?;
        let resolved_did = resolved.did.as_str();
        if resolved_did != member_did {
            return Err(Error::InvalidRecord(
                "member DID changed during repair".into(),
            ));
        }
        let approval = match &resolved.verification {
            crate::resolve::VerificationState::Verified { .. } => {
                prior.members[position].unverified_key_approval.clone()
            }
            crate::resolve::VerificationState::Unverified => {
                let expected = crate::crypto::unverified_key_approval(
                    prior.opake_version,
                    workspace_id.as_str(),
                    member_did,
                    &crate::crypto::EncryptionKeyFields {
                        x25519_public_key: &resolved.x25519_public_key,
                        x25519_algo: &resolved.x25519_algo,
                        ml_kem_public_key: &resolved.ml_kem_public_key,
                        ml_kem_algo: &resolved.ml_kem_algo,
                    },
                );
                let recorded = prior.members[position]
                    .unverified_key_approval
                    .as_ref()
                    .and_then(|bytes| bytes.decode().ok())
                    .is_some_and(|bytes| bytes.as_slice() == expected);
                if recorded {
                    prior.members[position].unverified_key_approval.clone()
                } else if confirmed_unverified_keys == Some(expected) {
                    Some(crate::records::AtBytes::from_raw(&expected))
                } else {
                    return Err(Error::UnverifiedKeyApprovalRequired {
                        did: member_did.to_owned(),
                    });
                }
            }
        };
        // Resolution is network I/O. Do not write the snapshot fetched before
        // it: a concurrent removal or approval update must be re-derived,
        // never overwritten by stale repair work.
        let (live_uri, live_cid, _) = self.fetch_keyring_chain_head(workspace_id).await?;
        if live_uri != prior_uri || live_cid != prior_cid {
            return Err(Error::CasConflict(
                "keyring head changed while resolving member repair".into(),
            ));
        }
        let wrapped = crate::crypto::wrap_key(
            key,
            &PublicKeyBundle {
                x25519: &resolved.x25519_public_key,
                ml_kem: &resolved.ml_kem_public_key,
            },
            member_did,
            &crate::crypto::WrapContext::Keyring {
                uri: workspace_id.as_str(),
            },
            &mut self.rng,
        )?;
        let mut new_record = prior;
        new_record.members[position].wrapped_key = Some(wrapped);
        new_record.members[position].unverified_key_approval = approval;
        self.write_keyring_supersede(workspace_id, prior_uri, prior_cid, new_record)
            .await?;
        Ok(WorkspaceMemberWriteResult {
            verification_notice: RecipientVerificationNotice {
                did: resolved_did.to_owned(),
                verification: resolved.verification,
            },
        })
    }

    /// Record an explicit approval for an admitted unverified member without
    /// supplying a group-key wrap. This lets another manager with the current
    /// key perform the later same-rotation repair, while the approval itself
    /// remains an ordinary manager-authorized head mutation.
    pub async fn approve_pending_workspace_member(
        &mut self,
        workspace_id: &WorkspaceId,
        member_did: &str,
        confirmed_unverified_keys: [u8; 32],
    ) -> Result<WorkspaceMemberWriteResult, Error> {
        let (prior_uri, prior_cid, prior) = self.fetch_keyring_chain_head(workspace_id).await?;
        self.require_manager(&prior)?;
        let position = prior
            .members
            .iter()
            .position(|member| member.did() == member_did)
            .ok_or_else(|| Error::NotFound(format!("no member entry for DID {member_did}")))?;
        let resolved = self.resolve_identity(member_did).await?;
        if !matches!(
            &resolved.verification,
            crate::resolve::VerificationState::Unverified
        ) {
            return Err(Error::InvalidRecord(
                "verified members do not need unverified-key approval".into(),
            ));
        }
        let expected = crate::crypto::unverified_key_approval(
            prior.opake_version,
            workspace_id.as_str(),
            member_did,
            &crate::crypto::EncryptionKeyFields {
                x25519_public_key: &resolved.x25519_public_key,
                x25519_algo: &resolved.x25519_algo,
                ml_kem_public_key: &resolved.ml_kem_public_key,
                ml_kem_algo: &resolved.ml_kem_algo,
            },
        );
        if confirmed_unverified_keys != expected {
            return Err(Error::UnverifiedKeyApprovalRequired {
                did: member_did.to_owned(),
            });
        }
        let (live_uri, live_cid, _) = self.fetch_keyring_chain_head(workspace_id).await?;
        if live_uri != prior_uri || live_cid != prior_cid {
            return Err(Error::CasConflict(
                "keyring head changed while approving member keys".into(),
            ));
        }
        let mut new_record = prior;
        new_record.members[position].unverified_key_approval =
            Some(crate::records::AtBytes::from_raw(&expected));
        self.write_keyring_supersede(workspace_id, prior_uri, prior_cid, new_record)
            .await?;
        Ok(WorkspaceMemberWriteResult {
            verification_notice: RecipientVerificationNotice {
                did: resolved.did,
                verification: resolved.verification,
            },
        })
    }

    /// Re-derive and repair missing current member wraps for every workspace
    /// where this account is presently a manager and holds the live group key.
    ///
    /// This unattended path never supplies a confirmation. Each candidate is
    /// re-read by [`Self::repair_workspace_member_wrap`], which permits a
    /// verified bundle or an unchanged recorded approval only, and rejects a
    /// head that changed while the recipient was resolved. Work that cannot
    /// run remains represented by the missing head wrap for a later pass.
    /// Deferrals are in-memory scheduling hints keyed by the observed head:
    /// a changed head or an explicit repair always re-evaluates authority.
    // spec:background-work § Remaining work is derived from records, never stored
    pub async fn sweep_member_wrap_repairs(&mut self) -> Result<MemberWrapRepairOutcome, Error> {
        let pass_started = (self.now_micros_fn)();
        let discovery_timer = self.identity_sleep_fn.clone();
        let discovery = self.discover_member_workspaces();
        let Some(discovery) = within_member_wrap_budget(
            discovery_timer,
            Duration::from_micros(MEMBER_WRAP_REPAIR_PASS_BUDGET_MICROS),
            discovery,
        )
        .await
        else {
            return Ok(MemberWrapRepairOutcome {
                discovery_deferred: true,
                ..MemberWrapRepairOutcome::default()
            });
        };
        let workspaces = discovery?;
        let private_keys = self.private_keys_from_cache();
        let mut outcome = MemberWrapRepairOutcome::default();
        let now = pass_started;
        let live_heads: HashSet<String> = workspaces
            .iter()
            .map(|workspace| workspace.uri.clone())
            .collect();
        retain_live_member_wrap_deferrals(&mut self.member_wrap_human_deferrals, &live_heads);
        self.member_wrap_visibility_backoff
            .retain(|head, _| live_heads.contains(head));
        let mut repair_budget = MAX_MEMBER_WRAP_REPAIRS_PER_PASS;
        let mut workspaces = workspaces;
        if !workspaces.is_empty() {
            rotate_member_wrap_workspaces(&mut workspaces, &mut self.member_wrap_workspace_cursor);
        }

        for workspace in workspaces {
            let workspace_id = workspace.workspace_id();
            if !workspace
                .record
                .members
                .iter()
                .any(|member| member.did() == self.did && matches!(member.role, Role::Manager))
            {
                outcome.skipped_not_manager += 1;
                continue;
            }
            let key = match Self::try_unwrap_workspace_key(
                &workspace.record.members,
                &self.did,
                workspace_id.as_str(),
                &private_keys.bundle(),
                workspace.record.opake_version,
            ) {
                Ok(Some(key)) => key,
                Ok(None) | Err(_) => {
                    outcome.skipped_without_current_key += 1;
                    continue;
                }
            };
            let missing_members: Vec<String> = workspace
                .record
                .members
                .iter()
                .filter(|member| member.wrapped_key.is_none())
                .map(|member| member.did().to_owned())
                .collect();

            let head_uri = workspace.uri.clone();
            if member_wrap_deferral_is_active(&self.member_wrap_visibility_backoff, &head_uri, now)
            {
                outcome.deferred_visibility += missing_members.len();
                continue;
            }

            for (member_index, member_did) in missing_members.iter().enumerate() {
                if repair_budget == 0
                    || (self.now_micros_fn)().saturating_sub(now)
                        >= MEMBER_WRAP_REPAIR_PASS_BUDGET_MICROS
                {
                    outcome.deferred_by_budget += missing_members.len() - member_index;
                    break;
                }
                let member_key = format!("{head_uri}\u{1f}{member_did}");
                if member_wrap_deferral_is_active(
                    &self.member_wrap_human_deferrals,
                    &member_key,
                    now,
                ) {
                    outcome.deferred_human_decision += 1;
                    continue;
                }
                repair_budget -= 1;
                outcome.attempted += 1;
                let remaining = MEMBER_WRAP_REPAIR_PASS_BUDGET_MICROS
                    .saturating_sub((self.now_micros_fn)().saturating_sub(now));
                let timer = self.identity_sleep_fn.clone();
                let repair =
                    self.repair_workspace_member_wrap(&workspace_id, &key, member_did, None);
                let repair_result =
                    within_member_wrap_budget(timer, Duration::from_micros(remaining), repair)
                        .await;
                let Some(repair_result) = repair_result else {
                    outcome.deferred_by_budget += missing_members.len() - member_index;
                    break;
                };
                match repair_result {
                    Ok(write) => {
                        outcome.repaired += 1;
                        outcome.verification_notices.push(write.verification_notice);
                    }
                    Err(Error::UnverifiedKeyApprovalRequired { .. }) => {
                        outcome.awaiting_approval += 1;
                        self.member_wrap_human_deferrals.insert(
                            member_key,
                            now.saturating_add(MEMBER_WRAP_HUMAN_DECISION_BACKOFF_MICROS),
                        );
                    }
                    Err(Error::VerificationFailed(_)) => outcome.verification_failed += 1,
                    Err(Error::VisibilityTimeout { .. }) => {
                        outcome.deferred_visibility += missing_members.len() - member_index;
                        self.member_wrap_visibility_backoff.insert(
                            head_uri.clone(),
                            now.saturating_add(MEMBER_WRAP_VISIBILITY_BACKOFF_MICROS),
                        );
                        break;
                    }
                    // A changed head, including a removed member, is never
                    // rebased by the runner. The next pass re-derives it.
                    Err(Error::CasConflict(_)) | Err(Error::NotFound(_)) => outcome.stale += 1,
                    Err(error) => {
                        outcome.errors += 1;
                        log::warn!(
                            "member-wrap-repair: workspace={} member={} failed: {error}",
                            workspace_id,
                            member_did
                        );
                    }
                }
            }
        }

        self.auto_persist_session().await?;
        Ok(outcome)
    }

    /// Leave a workspace via self-removal keyring supersede.
    ///
    /// Any member can author, not just managers — the indexer's keyring
    /// authority admits a non-manager supersede iff the only membership
    /// change is the author dropping themselves, with the remaining
    /// members' roles and wraps carried verbatim.
    ///
    /// Deliberately no key rotation. The leaver would have to mint the
    /// new group key themselves, so rotating here buys no forward
    /// secrecy — the leaver knows whatever key they wrap. Leave is
    /// cooperative departure; forward secrecy against the departed
    /// arrives with the next manager-authored rotation
    /// (`remove_workspace_member` covers the uncooperative case).
    pub async fn leave_workspace(
        &mut self,
        workspace_id: &WorkspaceId,
    ) -> Result<MutationOutcome, Error> {
        use crate::records::KeyringMember;

        let (prior_uri, prior_cid, prior) = self.fetch_keyring_chain_head(workspace_id).await?;

        if !prior.members.iter().any(|m| m.did() == self.did) {
            return Err(Error::InvalidRecord(format!(
                "{} is not a member of this workspace",
                self.did
            )));
        }
        if prior.members.len() == 1 {
            return Err(Error::InvalidRecord(
                "cannot leave as the last member — workspace destruction is not supported".into(),
            ));
        }
        let self_is_manager = prior
            .members
            .iter()
            .any(|m| m.did() == self.did && matches!(m.role, Role::Manager));
        let manager_count = prior
            .members
            .iter()
            .filter(|m| matches!(m.role, Role::Manager))
            .count();
        if self_is_manager && manager_count == 1 {
            return Err(Error::InvalidRecord(
                "cannot leave as the only manager — promote another member first".into(),
            ));
        }

        let remaining: Vec<KeyringMember> = prior
            .members
            .iter()
            .filter(|m| m.did() != self.did)
            .cloned()
            .collect();

        let new_record = crate::records::Keyring {
            opake_version: prior.opake_version,
            algo: prior.algo.clone(),
            members: remaining,
            rotation: prior.rotation,
            key_history: prior.key_history.clone(),
            encrypted_metadata: prior.encrypted_metadata.clone(),
            supersedes: None,     // filled by write_keyring_supersede
            supersedes_cid: None, // filled by write_keyring_supersede
            lineage: None,        // filled by write_keyring_supersede
            created_at: String::new(),
            modified_at: None,
        };

        self.write_keyring_supersede(workspace_id, prior_uri, prior_cid, new_record)
            .await
    }

    /// Remove a member from a workspace via curatorial keyring supersede.
    ///
    /// Federation model: any manager can author. Rotates the group key
    /// (forward-secrecy contract — removed members retain access to
    /// historical docs but can't read anything wrapped under the new key),
    /// re-wraps the new key for each remaining member, and writes the
    /// supersede on caller's PDS.
    ///
    /// Returns `(new_group_key, new_rotation)` so the caller can update
    /// their in-memory `Workspace`. The supersede record carries the
    /// prior rotation's members in `keyHistory` for backward decryption.
    pub async fn remove_workspace_member(
        &mut self,
        workspace_id: &WorkspaceId,
        group_key: &ContentKey,
        member_did: &str,
    ) -> Result<WorkspaceMemberRemoval, Error> {
        use crate::crypto::{generate_content_key, wrap_key, WrapContext};
        use crate::records::{KeyHistoryEntry, KeyringMember};

        let (prior_uri, prior_cid, prior) = self.fetch_keyring_chain_head(workspace_id).await?;
        self.require_manager(&prior)?;

        if !prior.members.iter().any(|m| m.did() == member_did) {
            return Err(Error::InvalidRecord(format!(
                "{member_did} is not a member of this workspace"
            )));
        }

        // Forward-secrecy rotation: generate a new group key, re-wrap
        // for everyone except the departing member, push the prior
        // members snapshot into key_history so historical docs can
        // still be decrypted by remaining members.
        let new_group_key = generate_content_key(&mut self.rng);
        let new_rotation = prior.rotation + 1;

        let mut excluded_members = Vec::new();
        let mut verification_notices = Vec::new();
        let mut new_members: Vec<KeyringMember> = Vec::with_capacity(prior.members.len() - 1);
        for prior_member in prior
            .members
            .iter()
            .filter(|member| member.did() != member_did)
        {
            // Begin from the actual head entry. This retains DID, role and any
            // key-bound approval, then clears the old generation's wrap before
            // attempting a fresh resolution.
            let mut member = prior_member.clone();
            member.wrapped_key = None;
            // The author already possesses their local identity keys. Their
            // self-wrap does not grant access to another account and must not
            // be withheld merely because their public record is unverified:
            // genesis correctly carries no invented counterparty approval.
            if member.did() == self.did {
                let x25519 = self.identity.x25519_public_key_bytes()?;
                let ml_kem = self.identity.ml_kem_public_key_bytes()?;
                member.wrapped_key = Some(wrap_key(
                    &new_group_key,
                    &PublicKeyBundle {
                        x25519: &x25519,
                        ml_kem: &ml_kem,
                    },
                    member.did(),
                    &WrapContext::Keyring {
                        uri: workspace_id.as_str(),
                    },
                    &mut self.rng,
                )?);
                new_members.push(member);
                continue;
            }
            let timer = self.identity_sleep_fn.clone();
            let resolution = within_member_wrap_budget(
                timer,
                Duration::from_micros(MEMBER_REMOVAL_RECIPIENT_RESOLUTION_BUDGET_MICROS),
                self.resolve_identity(member.did()),
            )
            .await;
            let resolved = match resolution {
                Some(Ok(resolved)) => resolved,
                Some(Err(error)) => {
                    let reason = if matches!(error, Error::VerificationFailed(_)) {
                        ExcludedMemberReason::VerificationFailed
                    } else {
                        ExcludedMemberReason::ResolutionFailed
                    };
                    excluded_members.push(ExcludedMember {
                        did: member.did.clone(),
                        reason,
                    });
                    new_members.push(member);
                    continue;
                }
                None => {
                    excluded_members.push(ExcludedMember {
                        did: member.did.clone(),
                        reason: ExcludedMemberReason::ResolutionFailed,
                    });
                    new_members.push(member);
                    continue;
                }
            };
            let permitted = match &resolved.verification {
                crate::resolve::VerificationState::Verified { .. } => true,
                crate::resolve::VerificationState::Unverified => {
                    let expected = crate::crypto::unverified_key_approval(
                        prior.opake_version,
                        workspace_id.as_str(),
                        member.did(),
                        &crate::crypto::EncryptionKeyFields {
                            x25519_public_key: &resolved.x25519_public_key,
                            x25519_algo: &resolved.x25519_algo,
                            ml_kem_public_key: &resolved.ml_kem_public_key,
                            ml_kem_algo: &resolved.ml_kem_algo,
                        },
                    );
                    member
                        .unverified_key_approval
                        .as_ref()
                        .and_then(|approval| approval.decode().ok())
                        .is_some_and(|approval| approval.as_slice() == expected)
                }
            };
            verification_notices.push(RecipientVerificationNotice {
                did: resolved.did.clone(),
                verification: resolved.verification.clone(),
            });
            if !permitted {
                excluded_members.push(ExcludedMember {
                    did: member.did.clone(),
                    reason: ExcludedMemberReason::ApprovalRequired,
                });
                new_members.push(member);
                continue;
            }
            member.wrapped_key = Some(wrap_key(
                &new_group_key,
                &PublicKeyBundle {
                    x25519: &resolved.x25519_public_key,
                    ml_kem: &resolved.ml_kem_public_key,
                },
                member.did(),
                &WrapContext::Keyring {
                    uri: workspace_id.as_str(),
                },
                &mut self.rng,
            )?);
            new_members.push(member);
        }

        let mut new_record = crate::records::Keyring {
            opake_version: prior.opake_version,
            algo: prior.algo.clone(),
            members: new_members,
            rotation: new_rotation,
            key_history: prior.key_history.clone(),
            encrypted_metadata: prior.encrypted_metadata.clone(),
            supersedes: None,     // filled by write_keyring_supersede
            supersedes_cid: None, // filled by write_keyring_supersede
            lineage: None,        // filled by write_keyring_supersede
            created_at: String::new(),
            modified_at: None,
        };

        // Push the prior rotation into key_history so docs encrypted
        // under it remain decryptable by remaining members.
        new_record.key_history.push(KeyHistoryEntry {
            rotation: prior.rotation,
            members: prior
                .members
                .iter()
                .filter(|member| member.did() != member_did)
                .cloned()
                .collect(),
        });

        // Re-encrypt metadata under new group key so old wrap is not
        // referenced after rotation.
        let anchor = crate::crypto::SealContext::new(
            workspace_id.as_str(),
            crate::crypto::SealType::KeyringMetadata,
        );
        let metadata: crate::crypto::KeyringMetadata =
            crate::crypto::decrypt_metadata(group_key, &prior.encrypted_metadata, &anchor)?;
        new_record.encrypted_metadata =
            crate::crypto::encrypt_metadata(&new_group_key, &metadata, &anchor, &mut self.rng)?;

        self.write_keyring_supersede(workspace_id, prior_uri, prior_cid, new_record)
            .await?;
        if !excluded_members.is_empty() {
            log::warn!(
                "workspace rotation excluded {} members without a new wrap",
                excluded_members.len()
            );
        }
        Ok(WorkspaceMemberRemoval {
            group_key: new_group_key,
            rotation: new_rotation,
            excluded_members,
            verification_notices,
        })
    }

    /// Update workspace metadata (name, description, icon) via keyring
    /// supersede.
    ///
    /// Manager-only; client-side check produces a clear error before the
    /// write attempt. The new record carries the prior members + rotation
    /// untouched — only `encrypted_metadata` changes.
    pub async fn update_workspace_metadata(
        &mut self,
        workspace_id: &WorkspaceId,
        group_key: &ContentKey,
        name: Option<&str>,
        description: Option<&str>,
        icon: Option<&str>,
    ) -> Result<MutationOutcome, Error> {
        use crate::crypto::{self, KeyringMetadata};

        let (prior_uri, prior_cid, prior) = self.fetch_keyring_chain_head(workspace_id).await?;
        self.require_manager(&prior)?;

        let anchor =
            crypto::SealContext::new(workspace_id.as_str(), crypto::SealType::KeyringMetadata);
        let mut metadata: KeyringMetadata =
            crypto::decrypt_metadata(group_key, &prior.encrypted_metadata, &anchor)?;
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

        let new_encrypted = crypto::encrypt_metadata(group_key, &metadata, &anchor, &mut self.rng)?;

        let mut new_record = prior;
        new_record.encrypted_metadata = new_encrypted;

        self.write_keyring_supersede(workspace_id, prior_uri, prior_cid, new_record)
            .await
    }

    /// Update a workspace member's role via keyring supersede.
    ///
    /// Manager-only. Other members' wrapped keys + roles carry forward
    /// untouched.
    pub async fn update_member_role(
        &mut self,
        workspace_id: &WorkspaceId,
        member_did: &str,
        new_role: Role,
    ) -> Result<MutationOutcome, Error> {
        let (prior_uri, prior_cid, mut prior) = self.fetch_keyring_chain_head(workspace_id).await?;
        self.require_manager(&prior)?;

        let member = prior
            .members
            .iter_mut()
            .find(|m| m.did() == member_did)
            .ok_or_else(|| {
                Error::InvalidRecord(format!("{member_did} is not a member of this workspace"))
            })?;
        member.role = new_role;

        self.write_keyring_supersede(workspace_id, prior_uri, prior_cid, prior)
            .await
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
    ) -> Result<
        (
            String,
            crate::crypto::DocumentMetadata,
            String,
            Option<String>,
        ),
        Error,
    > {
        let private_keys = self.private_keys_from_cache();
        crate::documents::resolve_grant_metadata(
            self.client.transport(),
            &private_keys.bundle(),
            grant_uri,
        )
        .await
    }

    /// Unwrap a workspace key from keyring member data (pure crypto, no network).
    ///
    /// `lineage_anchor` is the URI the member wraps were bound to — the
    /// workspace's stable genesis URI, resolved via [`Keyring::lineage_anchor`].
    /// Passing the head URI of a superseded keyring fails the AEAD check.
    pub fn unwrap_workspace_key(
        members: &[crate::records::KeyringMember],
        did: &str,
        lineage_anchor: &str,
        private_keys: &crate::crypto::PrivateKeyBundle<'_>,
        declared_version: u32,
    ) -> Result<ContentKey, Error> {
        Self::try_unwrap_workspace_key(
            members,
            did,
            lineage_anchor,
            private_keys,
            declared_version,
        )?
        .ok_or_else(|| Error::CurrentGroupKeyUnavailable {
            workspace_id: lineage_anchor.to_owned(),
        })
    }

    /// Like [`Self::unwrap_workspace_key`], but models an admitted member
    /// without a current wrap as historical-only rather than as a corrupted
    /// record. Callers must still derive rotation 0 before adopting it.
    pub fn try_unwrap_workspace_key(
        members: &[crate::records::KeyringMember],
        did: &str,
        lineage_anchor: &str,
        private_keys: &crate::crypto::PrivateKeyBundle<'_>,
        declared_version: u32,
    ) -> Result<Option<ContentKey>, Error> {
        let member = members
            .iter()
            .find(|m| m.did() == did)
            .ok_or_else(|| Error::NotFound(format!("no member entry for DID {did}")))?;
        let Some(wrapped_key) = member.wrapped_key.as_ref() else {
            return Ok(None);
        };
        Ok(Some(crate::crypto::unwrap_key(
            wrapped_key,
            private_keys,
            &crate::crypto::WrapContext::Keyring {
                uri: lineage_anchor,
            },
            // Transcript derives from the keyring's own declared version.
            declared_version,
        )?))
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
    ///    user's `at.opake.accountConfig` record. Seeded best-effort by
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
    pub async fn list_inbox(
        &mut self,
    ) -> Result<Vec<crate::indexer::types::IndexerEnvelope<crate::records::Grant>>, Error> {
        let signing_key = self.require_signing_key()?;
        let url = self.resolve_indexer_url();
        crate::indexer::fetch_inbox_all(self.client.transport(), &url, &self.did, &signing_key)
            .await
    }

    /// Fetch all incoming grants plus the references skipped as unreadable, so
    /// the inbox keeper can signal poison grants distinctly.
    pub async fn list_inbox_detailed(&mut self) -> Result<crate::indexer::InboxFetch, Error> {
        let signing_key = self.require_signing_key()?;
        let url = self.resolve_indexer_url();
        crate::indexer::fetch_inbox_all_detailed(
            self.client.transport(),
            &url,
            &self.did,
            &signing_key,
        )
        .await
    }

    /// Fetch all workspaces the user is a member of. Each envelope's
    /// `record` is the current keyring chain head, including the full
    /// member list.
    pub async fn discover_member_workspaces(
        &mut self,
    ) -> Result<Vec<crate::indexer::types::IndexerEnvelope<crate::records::Keyring>>, Error> {
        let signing_key = self.require_signing_key()?;
        let url = self.resolve_indexer_url();
        crate::indexer::fetch_member_workspaces(
            self.client.transport(),
            &url,
            &self.did,
            &signing_key,
        )
        .await
    }

    /// Like [`discover_member_workspaces`] but keeps the keyring references
    /// skipped as unreadable, so the workspace keeper can carry a distinct
    /// exists-but-unreadable signal.
    ///
    /// [`discover_member_workspaces`]: Self::discover_member_workspaces
    pub async fn discover_member_workspaces_detailed(
        &mut self,
    ) -> Result<crate::indexer::MemberWorkspacesFetch, Error> {
        let signing_key = self.require_signing_key()?;
        let url = self.resolve_indexer_url();
        crate::indexer::fetch_member_workspaces_detailed(
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
    pub(crate) fn require_signing_key(&self) -> Result<crate::storage::Ed25519SecretKey, Error> {
        self.identity
            .signing_key_bytes()?
            .ok_or_else(|| Error::Auth("identity is missing Ed25519 signing key".into()))
    }

    // -- Sharing (pending shares) --

    /// List pending (queued) outgoing shares.
    ///
    /// The authorized DID lives inside each intent's encrypted metadata, so
    /// exposing it costs a content-key fetch per document. Entries whose DID
    /// cannot be read are still listed, carrying the cause: an owner deciding
    /// whether to cancel a queued share needs to know whether the PDS was
    /// unreachable, the key is gone, or the intent itself is corrupt.
    ///
    // spec:sharing-grants § A share to a not-yet-ready recipient is queued, not dropped
    pub async fn list_pending_shares(
        &mut self,
    ) -> Result<Vec<crate::sharing::PendingShareEntry>, Error> {
        let private_keys = self.private_keys_from_cache();
        let mut result = crate::sharing::list_pending_shares(&mut self.client).await;
        if let Ok(entries) = &mut result {
            // Several intents routinely queue against the same document, and
            // each content-key fetch is its own PDS round trip.
            let mut content_keys: std::collections::HashMap<
                String,
                Result<crate::crypto::ContentKey, String>,
            > = std::collections::HashMap::new();
            for entry in entries {
                if !content_keys.contains_key(&entry.document) {
                    let fetched = crate::documents::fetch_content_key(
                        &mut self.client,
                        &self.did,
                        &private_keys.bundle(),
                        &entry.document,
                    )
                    .await
                    .map_err(|error| describe_content_key_failure(&error));
                    content_keys.insert(entry.document.clone(), fetched);
                }
                let content_key = match &content_keys[&entry.document] {
                    Ok(content_key) => content_key,
                    Err(reason) => {
                        entry.recipient_did_error = Some(reason.clone());
                        continue;
                    }
                };
                let context = crate::crypto::SealContext::new(
                    &entry.document,
                    crate::crypto::SealType::PendingShareMetadata,
                );
                match crate::crypto::decrypt_metadata::<crate::crypto::PendingShareMetadata>(
                    content_key,
                    &entry.encrypted_metadata,
                    &context,
                ) {
                    Ok(metadata) => entry.recipient_did = Some(metadata.recipient_did),
                    Err(_) => {
                        entry.recipient_did_error =
                            Some("the queued intent metadata could not be decrypted".into());
                    }
                }
            }
        }
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
    /// The caller starts from only a document URI. We peek the document's
    /// keyring reference to learn the workspace it belongs to, resolve that
    /// workspace by walking its keyring chain to the current head (which is
    /// where membership for anyone added by a supersede actually lives), then
    /// download using the resolved group keys. Returns the plaintext plus the
    /// keyring rkey and rotation so the caller can cache the group key for
    /// subsequent downloads.
    pub async fn download_as_workspace_member(
        &mut self,
        document_uri: &str,
    ) -> Result<KeyringDownloadResult, Error> {
        // Peek the keyring reference (stable workspace id) the document was
        // encrypted against, then resolve that workspace at its current head.
        let (workspace_id, doc_rotation) =
            crate::documents::fetch_document_keyring_ref(self.client.transport(), document_uri)
                .await?;
        let workspace_id = WorkspaceId::from_resolved(workspace_id);
        let (head_uri, _, _head) = self.fetch_keyring_chain_head(&workspace_id).await?;
        let ws = self.resolve_workspace_by_uri(&head_uri).await?;

        let (filename, plaintext) = crate::documents::download_keyring_document(
            self.client.transport(),
            ws.group_keys(),
            document_uri,
        )
        .await?;

        let keyring_rkey = atproto::parse_at_uri(workspace_id.as_str())?.rkey;
        let group_key = ws
            .key_for_rotation(doc_rotation)
            .ok_or_else(|| Error::CurrentGroupKeyUnavailable {
                workspace_id: workspace_id.as_str().to_owned(),
            })?
            .clone();
        Ok(KeyringDownloadResult {
            filename,
            plaintext,
            group_key,
            keyring_rkey,
            rotation: doc_rotation,
        })
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

    /// Resolve an entered handle or DID to its DID even when it has not yet
    /// published an Opake public key. Pending-share intent binds this DID;
    /// callers must never retain a mutable handle as authority.
    pub async fn resolve_recipient_did(&self, handle_or_did: &str) -> Result<String, Error> {
        let (did, _, _) =
            crate::resolve::resolve_pds_for_login(self.client.transport(), handle_or_did).await?;
        Ok(did)
    }

    /// Publish or update the caller's public key record on the PDS.
    pub async fn publish_public_key(&mut self) -> Result<String, Error> {
        let identity = &self.identity;
        let pubkey = identity.x25519_public_key_bytes()?;
        let mlkem_pubkey = identity.ml_kem_public_key_bytes()?;
        let signing_key = identity
            .signing_key_bytes()?
            .ok_or_else(|| Error::Auth("identity is missing Ed25519 signing key".into()))?;
        let signing_key = crate::crypto::Ed25519SigningKey::from_bytes(&signing_key);
        let now = self.now();
        let result = crate::resolve::publish_public_key(
            &mut self.client,
            &pubkey,
            &mlkem_pubkey,
            &signing_key,
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

    // -- Re-wrap sweep (background hygiene) --

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

/// Short owner-facing cause for a content key a listing could not obtain.
///
/// The distinction that matters to the owner is retry-shaped: a transport
/// failure is worth trying again, an unavailable key is not.
fn describe_content_key_failure(error: &Error) -> String {
    match error {
        Error::Xrpc { .. }
        | Error::Indexer { .. }
        | Error::Auth(_)
        | Error::NotFound(_)
        | Error::VisibilityTimeout { .. } => {
            format!("the document record could not be fetched: {error}")
        }
        _ => format!("the document content key is unavailable: {error}"),
    }
}

#[cfg(test)]
#[path = "opake_tests.rs"]
mod tests;
