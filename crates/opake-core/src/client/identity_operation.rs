//! Transient authorization for mutations to an account's DID document.
//!
//! This deliberately owns neither an [`super::Session`] nor an XRPC client:
//! those types persist and refresh standing credentials. An identity grant has
//! a shorter life and must disappear with this operation.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;

use futures_channel::oneshot;
use futures_util::future::{select, Either, FutureExt};
use zeroize::{Zeroize, Zeroizing};

use crate::crypto::{CryptoRng, RngCore};
use crate::error::Error;

use super::did::{encode_ed25519_did_key, resolve_did_document, resolve_plc_state};
use super::dpop::DpopKeyPair;
use super::oauth_discovery::{
    discover_authorization_server, generate_pkce, AuthorizationServerMetadata, PkceChallenge,
};
use super::oauth_token::{
    authenticated_json_post, build_authorization_url, build_client_id, exchange_code_unvalidated,
    pushed_authorization_request, revoke_temporary_token, validate_token_response,
};
use super::transport::Transport;

/// The pre-token authorization window. The server still determines a grant's
/// lifetime; a later confirmation driver must separately race its live grant
/// against this same finite deadline and cleanup on expiry.
pub const IDENTITY_OPERATION_DEADLINE: Duration = Duration::from_secs(10 * 60);

/// Per request bound used for authorization and later cleanup calls.
pub const IDENTITY_NETWORK_TIMEOUT: Duration = Duration::from_secs(5);

/// Platform-provided sleep, shared by operation holders. Native callers use a
/// runtime timer and WASM callers use `setTimeout`; core never assumes either.
pub type IdentitySleepFn = Rc<dyn Fn(Duration) -> Pin<Box<dyn Future<Output = ()>>>>;

/// The only cloneable part of an identity operation. It can request
/// abandonment while an async operation method is suspended, but exposes no
/// authorization material.
struct CancellationState {
    cancelled: Cell<bool>,
    waiter: RefCell<Option<oneshot::Sender<()>>>,
}

#[derive(Clone)]
pub struct IdentityOperationCancellation(Rc<CancellationState>);

impl IdentityOperationCancellation {
    /// Prevent any subsequent authorization, signing, or submission step.
    pub fn cancel(&self) {
        self.0.cancelled.set(true);
        if let Some(waiter) = self.0.waiter.borrow_mut().take() {
            let _ = waiter.send(());
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.get()
    }
}

/// The intended change to the account's `#opake` PLC verification method.
/// It contains public key material only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationMethodChange {
    Publish([u8; 32]),
    Remove,
}

/// Owner-supplied confirmation for the signer. It is accepted only by the
/// operation driver and zeroized when consumed or dropped.
pub struct OwnerConfirmation(Zeroizing<String>);

impl OwnerConfirmation {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// What the platform knows about the signer's owner-confirmation path. A
/// transport timeout must be reported as unknown rather than invented as a
/// delivery failure; holders supply a delivery failure only when their
/// confirmation channel positively establishes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OwnerConfirmationFailure {
    #[error("owner refused confirmation")]
    Refused,
    #[error("confirmation delivery failed")]
    DeliveryFailed,
    #[error("confirmation delivery is unknown")]
    DeliveryUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OwnerConfirmationWaitError {
    #[error(transparent)]
    Owner(#[from] OwnerConfirmationFailure),
    #[error("identity operation canceled")]
    Canceled,
    #[error("identity operation timed out")]
    TimedOut,
}

/// Callback data supplied by the same-origin authorization callback. It does
/// not expose the temporary grant returned by the authorization server.
pub struct IdentityCallback {
    code: Zeroizing<String>,
    state: Zeroizing<String>,
    issuer: String,
}

impl IdentityCallback {
    pub fn new(code: String, state: String, issuer: String) -> Self {
        Self {
            code: Zeroizing::new(code),
            state: Zeroizing::new(state),
            issuer,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum IdentityCleanup {
    Attempted,
    Failed,
    Unavailable,
}

/// The mutation outcome known to this client. `Submitted` records a successful
/// response from the submit endpoint; it does not claim that a later DID read
/// was caused by this operation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum IdentityMutation {
    Submitted,
    Refused { reason: IdentityRefusal },
    Canceled,
    Unknown,
}

/// The furthest safe protocol stage reached by a refusal. These values avoid
/// exposing raw transport errors, which can contain server-provided details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum IdentityRefusal {
    GrantRejected,
    ConfirmationRequestRefused,
    ConfirmationRefused,
    ConfirmationDeliveryFailed,
    ConfirmationDeliveryUnknown,
    PreparationFailed,
    SignerRefused,
    SignerResponseUnknown,
}

/// A fresh DID-state observation made only after a submitted operation lost
/// its response. It deliberately says nothing about which operation caused
/// the observed state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum IdentityReconciliation {
    ObservedMatching,
    ObservedUnchanged,
    ObservedConflict,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct IdentityOperationResult {
    pub mutation: IdentityMutation,
    /// Present only when a sent submission did not return a usable response.
    pub reconciliation: Option<IdentityReconciliation>,
    pub cleanup: IdentityCleanup,
}

/// An unpersisted authorization attempt for one DID operation.
///
/// There is intentionally no `Clone`, `Debug`, `Serialize`, or credential
/// accessor on this type. The PKCE verifier and DPoP private key are owned
/// here until completion or drop.
pub struct IdentityOperation<T: Transport> {
    transport: T,
    pds_url: String,
    did: String,
    redirect_uri: String,
    change: VerificationMethodChange,
    dpop_key: DpopKeyPair,
    pkce: PkceChallenge,
    state: Zeroizing<String>,
    dpop_nonce: Option<String>,
    authorization_server: Option<AuthorizationServerMetadata>,
    deadline_micros: u64,
    now_micros: fn() -> u64,
    sleep: IdentitySleepFn,
    cancellation: IdentityOperationCancellation,
    terminal: Cell<bool>,
    submission_started: Cell<bool>,
    pre_submit_opake: Option<[u8; 32]>,
}

pub struct IdentityOperationConfig {
    pub pds_url: String,
    pub did: String,
    pub redirect_uri: String,
    pub change: VerificationMethodChange,
    pub now_micros: fn() -> u64,
    pub sleep: IdentitySleepFn,
}

impl<T: Transport> Drop for IdentityOperation<T> {
    fn drop(&mut self) {
        self.discard_owned_secrets();
    }
}

impl<T: Transport> IdentityOperation<T> {
    /// Construct a live operation before any I/O, so callers can register the
    /// cancellation handle before discovery or PAR awaits.
    pub fn new(
        transport: T,
        config: IdentityOperationConfig,
        rng: &mut (impl CryptoRng + RngCore),
    ) -> (Self, IdentityOperationCancellation) {
        let mut state_bytes = [0u8; 32];
        rng.fill_bytes(&mut state_bytes);
        let state = Zeroizing::new(base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            state_bytes,
        ));
        let cancellation = IdentityOperationCancellation(Rc::new(CancellationState {
            cancelled: Cell::new(false),
            waiter: RefCell::new(None),
        }));
        let deadline_micros = (config.now_micros)().saturating_add(
            IDENTITY_OPERATION_DEADLINE
                .as_micros()
                .try_into()
                .expect("identity deadline fits u64"),
        );
        let operation = Self {
            transport,
            pds_url: config.pds_url,
            did: config.did,
            redirect_uri: config.redirect_uri,
            change: config.change,
            dpop_key: DpopKeyPair::generate(rng),
            pkce: generate_pkce(rng),
            state,
            dpop_nonce: None,
            authorization_server: None,
            deadline_micros,
            now_micros: config.now_micros,
            sleep: config.sleep,
            cancellation: cancellation.clone(),
            terminal: Cell::new(false),
            submission_started: Cell::new(false),
            pre_submit_opake: None,
        };
        (operation, cancellation)
    }

    /// Start OAuth discovery and PAR for the narrow `identity:*` grant.
    /// A canceled or timed-out operation never returns an authorization URL.
    /// This stage holds no issued credential; the later callback/confirmation
    /// driver owns the active cleanup obligation.
    pub async fn start_authorization(&mut self) -> Result<String, Error> {
        let result = self.start_authorization_live().await;
        if result.is_err() {
            // A PAR failure, cancellation, or timeout requires a completely
            // fresh operation, never reuse of this PKCE/DPoP/state tuple.
            self.terminal.set(true);
            self.discard_owned_secrets();
        }
        result
    }

    async fn start_authorization_live(&mut self) -> Result<String, Error> {
        if self.terminal.get() {
            return Err(Error::Auth("identity operation is already terminal".into()));
        }
        self.ensure_live()?;
        if self.authorization_server.is_some() {
            return Err(Error::Auth("identity authorization already started".into()));
        }

        let discovery = bounded(
            &self.sleep,
            discover_authorization_server(&self.transport, &self.pds_url),
        );
        let (protected_resource, authorization_server) = discovery.await??;
        self.validate_discovery_binding(&protected_resource, &authorization_server)?;
        self.ensure_live()?;

        let client_id = build_client_id(&self.redirect_uri);
        let mut rng = crate::crypto::OsRng;
        let par = bounded(
            &self.sleep,
            pushed_authorization_request(
                &self.transport,
                authorization_server.par_endpoint(),
                &client_id,
                &self.redirect_uri,
                &self.pkce,
                "atproto identity:*",
                self.state.as_str(),
                None,
                &self.dpop_key,
                &mut self.dpop_nonce,
                (self.now_micros)() as i64 / 1_000_000,
                &mut rng,
            ),
        )
        .await??;
        self.ensure_live()?;
        let url = build_authorization_url(
            &authorization_server.authorization_endpoint,
            &client_id,
            &par.request_uri,
        );
        self.authorization_server = Some(authorization_server);
        Ok(url)
    }

    /// Whether this operation is intended to publish or remove the method.
    pub fn change(&self) -> VerificationMethodChange {
        self.change
    }

    /// The account this opaque operation is bound to.
    pub fn did(&self) -> &str {
        &self.did
    }

    /// Race an owner-input future against cancellation and the operation
    /// deadline. This is the primitive a signer driver uses after it receives
    /// a temporary grant: it remains live without borrowing `Opake`, and the
    /// clone-only cancellation handle wakes this future immediately.
    pub async fn await_owner_confirmation<F>(
        &mut self,
        confirmation: F,
    ) -> Result<OwnerConfirmation, OwnerConfirmationWaitError>
    where
        F: Future<Output = Result<OwnerConfirmation, OwnerConfirmationFailure>>,
    {
        self.ensure_live()
            .map_err(|_| OwnerConfirmationWaitError::Canceled)?;
        let (sender, canceled) = oneshot::channel();
        *self.cancellation.0.waiter.borrow_mut() = Some(sender);
        // `cancel` can run between the first check and installing the wakeup.
        // Re-check after registration so that race never waits for the timer.
        if self.cancellation.is_cancelled() {
            self.cancellation.0.waiter.borrow_mut().take();
            return Err(OwnerConfirmationWaitError::Canceled);
        }
        let remaining_micros = self.deadline_micros.saturating_sub((self.now_micros)());
        let timer = (self.sleep)(Duration::from_micros(remaining_micros));
        let result = select(
            confirmation.boxed_local(),
            select(canceled.map(|_| ()), timer).boxed_local(),
        )
        .await;
        self.cancellation.0.waiter.borrow_mut().take();
        match result {
            Either::Left((confirmation, _)) => {
                confirmation.map_err(OwnerConfirmationWaitError::from)
            }
            Either::Right((Either::Left((_, _)), _)) => Err(OwnerConfirmationWaitError::Canceled),
            Either::Right((Either::Right((_, _)), _)) => {
                self.cancellation.cancel();
                Err(OwnerConfirmationWaitError::TimedOut)
            }
        }
    }

    /// Complete one fully driven identity mutation. The caller supplies the
    /// callback and a live owner-confirmation future; this method never hands
    /// out a token, always disposes received credentials before returning, and
    /// reports every handled post-token exit with its cleanup observation.
    pub async fn complete<F>(
        &mut self,
        callback: IdentityCallback,
        confirmation: F,
    ) -> Result<IdentityOperationResult, Error>
    where
        F: Future<Output = Result<OwnerConfirmation, OwnerConfirmationFailure>>,
    {
        if let Err(error) = self.ensure_live() {
            self.terminal.set(true);
            self.discard_owned_secrets();
            return Err(error);
        }
        if self.terminal.get() {
            return Err(Error::Auth("identity operation is already terminal".into()));
        }
        let server = self
            .authorization_server
            .clone()
            .ok_or_else(|| Error::Auth("identity authorization was not started".into()))?;
        if callback.state.as_str() != self.state.as_str() || callback.issuer != server.issuer {
            return Err(Error::Auth(
                "identity callback does not match this authorization attempt".into(),
            ));
        }
        self.terminal.set(true);
        let client_id = build_client_id(&self.redirect_uri);
        let mut rng = crate::crypto::OsRng;
        let token_exchange = bounded(
            &self.sleep,
            exchange_code_unvalidated(
                &self.transport,
                &server.token_endpoint,
                &client_id,
                callback.code.as_str(),
                &self.redirect_uri,
                &self.pkce.verifier,
                &self.dpop_key,
                &mut self.dpop_nonce,
                (self.now_micros)() as i64 / 1_000_000,
                &mut rng,
            ),
        )
        .await;
        let mut tokens = match token_exchange {
            Ok(Ok(tokens)) => tokens,
            Ok(Err(error)) | Err(error) => {
                self.discard_owned_secrets();
                return Err(error);
            }
        };
        let token_validation =
            validate_token_response(&tokens, Some(&self.did)).and_then(|()| {
                if tokens.scope.as_deref().is_some_and(|scope| {
                    scope.split_whitespace().any(|value| value == "identity:*")
                }) {
                    Ok(())
                } else {
                    Err(Error::Auth(
                        "identity token scope omitted identity:*".into(),
                    ))
                }
            });
        let (mutation, reconciliation) = match token_validation {
            Err(_) if self.cancellation.is_cancelled() => (IdentityMutation::Canceled, None),
            Err(_) => (
                IdentityMutation::Refused {
                    reason: IdentityRefusal::GrantRejected,
                },
                None,
            ),
            Ok(()) => match self
                .submit_with_tokens(&tokens.access_token, confirmation)
                .await
            {
                Ok(()) => (IdentityMutation::Submitted, None),
                Err(_) if self.submission_started.get() => {
                    // A request was already sent. A cancel or a transport
                    // timeout cannot establish whether the server applied it.
                    (IdentityMutation::Unknown, Some(self.reconcile().await))
                }
                Err(_) if self.cancellation.is_cancelled() => (IdentityMutation::Canceled, None),
                Err(reason) => (IdentityMutation::Refused { reason }, None),
            },
        };
        let cleanup = self.cleanup_tokens(&server, &mut tokens).await;
        tokens.zeroize();
        self.discard_owned_secrets();
        Ok(IdentityOperationResult {
            mutation,
            reconciliation,
            cleanup,
        })
    }

    async fn submit_with_tokens<F>(
        &mut self,
        access_token: &str,
        confirmation: F,
    ) -> Result<(), IdentityRefusal>
    where
        F: Future<Output = Result<OwnerConfirmation, OwnerConfirmationFailure>>,
    {
        self.ensure_live()
            .map_err(|_| IdentityRefusal::PreparationFailed)?;
        let mut rng = crate::crypto::OsRng;
        let request_url = format!(
            "{}/xrpc/com.atproto.identity.requestPlcOperationSignature",
            self.pds_url
        );
        let confirmation_request = bounded(
            &self.sleep,
            authenticated_json_post(
                &self.transport,
                &request_url,
                None,
                access_token,
                &self.dpop_key,
                &mut self.dpop_nonce,
                (self.now_micros)() as i64 / 1_000_000,
                &mut rng,
            ),
        )
        .await;
        let response = match confirmation_request {
            Ok(Ok(response)) => response,
            // Neither a missing response nor a transport failure tells us if
            // the confirmation channel accepted or delivered the request.
            Ok(Err(_)) | Err(_) => return Err(IdentityRefusal::ConfirmationDeliveryUnknown),
        };
        require_success(&response).map_err(|_| IdentityRefusal::ConfirmationRequestRefused)?;
        let confirmation = self
            .await_owner_confirmation(confirmation)
            .await
            .map_err(|error| match error {
                OwnerConfirmationWaitError::Owner(OwnerConfirmationFailure::Refused) => {
                    IdentityRefusal::ConfirmationRefused
                }
                OwnerConfirmationWaitError::Owner(OwnerConfirmationFailure::DeliveryFailed) => {
                    IdentityRefusal::ConfirmationDeliveryFailed
                }
                OwnerConfirmationWaitError::Owner(OwnerConfirmationFailure::DeliveryUnknown) => {
                    IdentityRefusal::ConfirmationDeliveryUnknown
                }
                OwnerConfirmationWaitError::Canceled | OwnerConfirmationWaitError::TimedOut => {
                    IdentityRefusal::PreparationFailed
                }
            })?;
        self.ensure_live()
            .map_err(|_| IdentityRefusal::PreparationFailed)?;
        let document = bounded(
            &self.sleep,
            resolve_did_document(&self.transport, &self.did),
        )
        .await
        .map_err(|_| IdentityRefusal::PreparationFailed)?
        .map_err(|_| IdentityRefusal::PreparationFailed)?;
        let current = document
            .opake_key()
            .map_err(|_| IdentityRefusal::PreparationFailed)?;
        match (self.change, current) {
            (VerificationMethodChange::Publish(_), Some(_))
            | (VerificationMethodChange::Remove, None) => {
                return Err(IdentityRefusal::PreparationFailed);
            }
            _ => {}
        }
        self.pre_submit_opake = current;
        let state = bounded(&self.sleep, resolve_plc_state(&self.transport, &self.did))
            .await
            .map_err(|_| IdentityRefusal::PreparationFailed)?
            .map_err(|_| IdentityRefusal::PreparationFailed)?;
        let body = self
            .sign_request_body(state, confirmation.as_str())
            .map_err(|_| IdentityRefusal::PreparationFailed)?;
        self.ensure_live()
            .map_err(|_| IdentityRefusal::SignerRefused)?;
        let sign_url = format!(
            "{}/xrpc/com.atproto.identity.signPlcOperation",
            self.pds_url
        );
        let signer_response = bounded(
            &self.sleep,
            authenticated_json_post(
                &self.transport,
                &sign_url,
                Some(body),
                access_token,
                &self.dpop_key,
                &mut self.dpop_nonce,
                (self.now_micros)() as i64 / 1_000_000,
                &mut rng,
            ),
        )
        .await;
        let response = match signer_response {
            Ok(Ok(response)) => response,
            Ok(Err(_)) | Err(_) => return Err(IdentityRefusal::SignerResponseUnknown),
        };
        require_success(&response).map_err(|_| IdentityRefusal::SignerRefused)?;
        let operation = serde_json::from_slice::<serde_json::Value>(&response.body)
            .map_err(|_| IdentityRefusal::SignerResponseUnknown)?
            .get("operation")
            .cloned()
            .ok_or(IdentityRefusal::SignerResponseUnknown)?;
        self.ensure_live()
            .map_err(|_| IdentityRefusal::SignerRefused)?;
        let submit_url = format!(
            "{}/xrpc/com.atproto.identity.submitPlcOperation",
            self.pds_url
        );
        self.submission_started.set(true);
        let response = bounded(
            &self.sleep,
            authenticated_json_post(
                &self.transport,
                &submit_url,
                Some(serde_json::json!({"operation": operation})),
                access_token,
                &self.dpop_key,
                &mut self.dpop_nonce,
                (self.now_micros)() as i64 / 1_000_000,
                &mut rng,
            ),
        )
        .await
        .map_err(|_| IdentityRefusal::SignerRefused)?
        .map_err(|_| IdentityRefusal::SignerRefused)?;
        require_success(&response).map_err(|_| IdentityRefusal::SignerRefused)
    }

    async fn reconcile(&self) -> IdentityReconciliation {
        let document = match bounded(
            &self.sleep,
            resolve_did_document(&self.transport, &self.did),
        )
        .await
        {
            Ok(Ok(document)) => document,
            _ => return IdentityReconciliation::Unavailable,
        };
        let current = match document.opake_key() {
            Ok(current) => current,
            Err(_) => return IdentityReconciliation::ObservedConflict,
        };
        match (self.change, current) {
            (VerificationMethodChange::Publish(expected), Some(current)) if expected == current => {
                IdentityReconciliation::ObservedMatching
            }
            (VerificationMethodChange::Remove, None) => IdentityReconciliation::ObservedMatching,
            (VerificationMethodChange::Publish(_), None) => {
                IdentityReconciliation::ObservedUnchanged
            }
            (VerificationMethodChange::Remove, Some(current))
                if Some(current) == self.pre_submit_opake =>
            {
                IdentityReconciliation::ObservedUnchanged
            }
            _ => IdentityReconciliation::ObservedConflict,
        }
    }

    fn sign_request_body(
        &self,
        mut state: serde_json::Value,
        token: &str,
    ) -> Result<serde_json::Value, Error> {
        let state = state
            .as_object_mut()
            .ok_or_else(|| Error::InvalidRecord("malformed PLC state".into()))?;
        let methods = state
            .get_mut("verificationMethods")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or_else(|| Error::InvalidRecord("PLC state omitted verificationMethods".into()))?;
        match self.change {
            VerificationMethodChange::Publish(key) => {
                if methods.contains_key("opake") {
                    return Err(Error::VerificationFailed(
                        "current PLC state already contains #opake".into(),
                    ));
                }
                methods.insert(
                    "opake".into(),
                    serde_json::Value::String(encode_ed25519_did_key(&key)),
                );
            }
            VerificationMethodChange::Remove => {
                methods.remove("opake");
            }
        }
        let methods = serde_json::Value::Object(methods.clone());
        let rotation_keys = state.get("rotationKeys").cloned();
        let also_known_as = state.get("alsoKnownAs").cloned();
        let services = state.get("services").cloned();
        Ok(
            serde_json::json!({"token": token, "rotationKeys": rotation_keys, "alsoKnownAs": also_known_as, "verificationMethods": methods, "services": services}),
        )
    }

    async fn cleanup_tokens(
        &mut self,
        server: &AuthorizationServerMetadata,
        tokens: &mut super::oauth_token::TokenResponse,
    ) -> IdentityCleanup {
        let Some(endpoint) = server.revocation_endpoint.as_deref() else {
            return IdentityCleanup::Unavailable;
        };
        let mut rng = crate::crypto::OsRng;
        let access = bounded(
            &self.sleep,
            revoke_temporary_token(
                &self.transport,
                endpoint,
                &tokens.access_token,
                &self.dpop_key,
                &mut self.dpop_nonce,
                (self.now_micros)() as i64 / 1_000_000,
                &mut rng,
            ),
        )
        .await;
        let refresh = match tokens.refresh_token.as_deref() {
            Some(token) => {
                bounded(
                    &self.sleep,
                    revoke_temporary_token(
                        &self.transport,
                        endpoint,
                        token,
                        &self.dpop_key,
                        &mut self.dpop_nonce,
                        (self.now_micros)() as i64 / 1_000_000,
                        &mut rng,
                    ),
                )
                .await
            }
            None => Ok(Ok(())),
        };
        if access.is_ok_and(|r| r.is_ok()) && refresh.is_ok_and(|r| r.is_ok()) {
            IdentityCleanup::Attempted
        } else {
            IdentityCleanup::Failed
        }
    }

    /// End an operation's local credential lifetime even if its owner keeps
    /// this opaque value around to inspect the result. The generated
    /// `Zeroize` implementations cover the redacted PKCE verifier and DPoP
    /// private key; state and nonce are also authorization material.
    fn discard_owned_secrets(&mut self) {
        self.pkce.zeroize();
        self.state.zeroize();
        self.dpop_nonce.zeroize();
        self.dpop_key.zeroize();
        self.authorization_server = None;
    }

    fn ensure_live(&self) -> Result<(), Error> {
        if self.cancellation.is_cancelled() {
            return Err(Error::Auth("identity operation canceled".into()));
        }
        if (self.now_micros)() >= self.deadline_micros {
            self.cancellation.cancel();
            return Err(Error::Auth("identity operation timed out".into()));
        }
        Ok(())
    }

    fn validate_discovery_binding(
        &self,
        protected_resource: &super::oauth_discovery::ProtectedResourceMetadata,
        authorization_server: &AuthorizationServerMetadata,
    ) -> Result<(), Error> {
        // OAuth metadata identifiers are URL values. This client permits only
        // a trailing-slash spelling difference; it deliberately does not
        // invent broader URL normalization for this account-binding check.
        if protected_resource.resource.trim_end_matches('/') != self.pds_url.trim_end_matches('/') {
            return Err(Error::Auth(
                "protected-resource metadata does not name this account PDS".into(),
            ));
        }
        let advertised = protected_resource
            .authorization_servers
            .first()
            .ok_or_else(|| {
                Error::Auth("protected-resource metadata has no authorization server".into())
            })?;
        if advertised.trim_end_matches('/') != authorization_server.issuer.trim_end_matches('/') {
            return Err(Error::Auth(
                "authorization-server metadata issuer differs from the advertised server".into(),
            ));
        }
        Ok(())
    }
}

async fn bounded<F>(sleep: &IdentitySleepFn, future: F) -> Result<F::Output, Error>
where
    F: Future,
{
    let request = future.boxed_local();
    let timer = sleep(IDENTITY_NETWORK_TIMEOUT).boxed_local();
    match select(request, timer).await {
        Either::Left((value, _)) => Ok(value),
        Either::Right((_, _)) => Err(Error::Auth("identity operation network timeout".into())),
    }
}

fn require_success(response: &super::transport::HttpResponse) -> Result<(), Error> {
    if (200..300).contains(&response.status) {
        Ok(())
    } else {
        Err(Error::Xrpc {
            status: response.status,
            message: "identity endpoint refused request".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{HttpResponse, RequestBody};
    use crate::test_utils::MockTransport;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    fn now() -> u64 {
        1_000_000
    }

    fn immediate_sleep() -> IdentitySleepFn {
        Rc::new(|_| Box::pin(async {}))
    }

    fn operation(
        transport: MockTransport,
    ) -> (
        IdentityOperation<MockTransport>,
        IdentityOperationCancellation,
    ) {
        let mut rng = crate::crypto::OsRng;
        IdentityOperation::new(
            transport,
            IdentityOperationConfig {
                pds_url: "https://pds.example.test".into(),
                did: "did:plc:alice".into(),
                redirect_uri: "https://app.example.test/callback".into(),
                change: VerificationMethodChange::Remove,
                now_micros: now,
                sleep: immediate_sleep(),
            },
            &mut rng,
        )
    }

    fn response(body: serde_json::Value) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn status_response(status: u16) -> HttpResponse {
        HttpResponse {
            status,
            headers: vec![],
            body: vec![],
        }
    }

    fn server(revocation_endpoint: Option<&str>) -> AuthorizationServerMetadata {
        AuthorizationServerMetadata {
            issuer: "https://auth.example.test".into(),
            authorization_endpoint: "".into(),
            token_endpoint: "https://auth.example.test/token".into(),
            revocation_endpoint: revocation_endpoint.map(str::to_owned),
            pushed_authorization_request_endpoint: None,
            scopes_supported: vec![],
            response_types_supported: vec![],
            grant_types_supported: vec![],
            code_challenge_methods_supported: vec![],
            dpop_signing_alg_values_supported: vec![],
            token_endpoint_auth_methods_supported: vec![],
            require_pushed_authorization_requests: false,
        }
    }

    fn ready_operation(
        transport: MockTransport,
    ) -> (
        IdentityOperation<MockTransport>,
        IdentityOperationCancellation,
    ) {
        let (mut operation, cancellation) = operation(transport);
        operation.authorization_server = Some(server(Some("https://auth.example.test/revoke")));
        (operation, cancellation)
    }

    fn token(subject: &str) -> HttpResponse {
        response(serde_json::json!({
            "access_token": "temporary-access",
            "refresh_token": "temporary-refresh",
            "token_type": "DPoP",
            "scope": "atproto identity:*",
            "sub": subject,
        }))
    }

    fn callback<T: Transport>(operation: &IdentityOperation<T>) -> IdentityCallback {
        IdentityCallback::new(
            "code".into(),
            operation.state.to_string(),
            "https://auth.example.test".into(),
        )
    }

    fn did_document_without_opake() -> HttpResponse {
        response(serde_json::json!({
            "id": "did:plc:alice",
            "verificationMethod": []
        }))
    }

    fn did_document_with_opake(key: [u8; 32]) -> HttpResponse {
        response(serde_json::json!({
            "id": "did:plc:alice",
            "verificationMethod": [{
                "id": "did:plc:alice#opake",
                "controller": "did:plc:alice",
                "type": "Multikey",
                "publicKeyMultibase": encode_ed25519_did_key(&key).strip_prefix("did:key:").unwrap()
            }]
        }))
    }

    fn plc_state() -> HttpResponse {
        response(serde_json::json!({
            "rotationKeys": ["did:key:zrotation"],
            "alsoKnownAs": ["at://alice.test"],
            "verificationMethods": {
                "atproto": "did:key:zexisting",
                "unrelated": "did:key:zunrelated"
            },
            "services": {
                "atproto_pds": {
                    "type": "AtprotoPersonalDataServer",
                    "endpoint": "https://pds.example.test"
                }
            }
        }))
    }

    fn plc_state_with_opake(key: [u8; 32]) -> HttpResponse {
        response(serde_json::json!({
            "rotationKeys": ["did:key:zrotation"],
            "alsoKnownAs": ["at://alice.test"],
            "verificationMethods": {
                "atproto": "did:key:zexisting",
                "opake": encode_ed25519_did_key(&key)
            },
            "services": {}
        }))
    }

    fn form_value(request: &super::super::transport::HttpRequest, name: &str) -> String {
        let Some(RequestBody::Form(params)) = request.body.as_ref() else {
            panic!("expected an OAuth form request");
        };
        params
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| panic!("OAuth form omitted {name}"))
    }

    fn dpop_jwk(request: &super::super::transport::HttpRequest) -> serde_json::Value {
        let proof = request
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("dpop"))
            .map(|(_, value)| value)
            .expect("OAuth request omitted DPoP proof");
        let header = proof.split('.').next().expect("DPoP proof omitted header");
        let bytes =
            base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, header)
                .expect("DPoP header was not base64url");
        serde_json::from_slice::<serde_json::Value>(&bytes).expect("DPoP header was not JSON")
            ["jwk"]
            .clone()
    }

    /// A controlled hanging revocation call. It still records and consumes
    /// its canned response so the following credential proves cleanup did not
    /// stop after the timed-out one.
    #[derive(Clone)]
    struct TimeoutFirstRevocationTransport {
        inner: MockTransport,
        timed_out: Arc<AtomicBool>,
    }

    impl TimeoutFirstRevocationTransport {
        fn new(inner: MockTransport) -> Self {
            Self {
                inner,
                timed_out: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl Transport for TimeoutFirstRevocationTransport {
        async fn send(
            &self,
            request: super::super::transport::HttpRequest,
        ) -> Result<HttpResponse, Error> {
            let timeout =
                request.url.ends_with("/revoke") && !self.timed_out.swap(true, Ordering::SeqCst);
            if timeout {
                let _ = self.inner.send(request).await?;
                futures_util::future::pending().await
            } else {
                self.inner.send(request).await
            }
        }
    }

    /// Delivers a valid token only after cancellation has won. This models a
    /// response already in flight, without granting the test a product-only
    /// escape hatch around the operation driver's cleanup path.
    #[derive(Clone)]
    struct CancelOnTokenResponseTransport {
        inner: MockTransport,
        cancellation: Rc<RefCell<Option<IdentityOperationCancellation>>>,
    }

    impl CancelOnTokenResponseTransport {
        fn new(inner: MockTransport) -> Self {
            Self {
                inner,
                cancellation: Rc::new(RefCell::new(None)),
            }
        }

        fn cancel_when_token_arrives(&self, cancellation: IdentityOperationCancellation) {
            *self.cancellation.borrow_mut() = Some(cancellation);
        }
    }

    impl Transport for CancelOnTokenResponseTransport {
        async fn send(
            &self,
            request: super::super::transport::HttpRequest,
        ) -> Result<HttpResponse, Error> {
            let is_token_response = request.url.ends_with("/token");
            let response = self.inner.send(request).await?;
            if is_token_response {
                if let Some(cancellation) = self.cancellation.borrow_mut().take() {
                    cancellation.cancel();
                }
            }
            Ok(response)
        }
    }

    #[tokio::test]
    async fn cancellation_can_be_registered_before_authorization_io() {
        let transport = MockTransport::new();
        let (mut operation, cancel) = operation(transport.clone());
        cancel.cancel();

        let error = operation.start_authorization().await.unwrap_err();
        assert!(error.to_string().contains("canceled"));
        assert!(transport.requests().is_empty());
    }

    #[tokio::test]
    async fn authorization_uses_fresh_identity_scope_and_keeps_state_in_holder() {
        let transport = MockTransport::new();
        transport.enqueue(response(serde_json::json!({
            "resource": "https://pds.example.test",
            "authorization_servers": ["https://auth.example.test"]
        })));
        transport.enqueue(response(serde_json::json!({
            "issuer": "https://auth.example.test",
            "authorization_endpoint": "https://auth.example.test/authorize",
            "token_endpoint": "https://auth.example.test/token",
            "pushed_authorization_request_endpoint": "https://auth.example.test/par"
        })));
        transport.enqueue(response(serde_json::json!({
            "request_uri": "urn:request:opaque",
            "expires_in": 60
        })));

        let (mut operation, _cancel) = operation(transport.clone());
        let authorization_url = operation.start_authorization().await.unwrap();

        assert!(authorization_url.contains("request_uri=urn%3Arequest%3Aopaque"));
        let requests = transport.requests();
        assert_eq!(requests.len(), 3);
        let RequestBody::Form(params) = requests[2].body.as_ref().unwrap() else {
            panic!("PAR must be form encoded");
        };
        assert_eq!(
            params.iter().find(|(name, _)| name == "scope").unwrap().1,
            "atproto identity:*"
        );
        let client_id = &params
            .iter()
            .find(|(name, _)| name == "client_id")
            .unwrap()
            .1;
        assert!(client_id.contains("identity%3A%2A"));
        assert!(!operation.did().is_empty());
    }

    #[tokio::test]
    async fn separate_identity_operations_use_fresh_state_pkce_and_dpop_keys() {
        let transport = MockTransport::new();
        for request_uri in ["urn:request:first", "urn:request:second"] {
            transport.enqueue(response(serde_json::json!({
                "resource": "https://pds.example.test",
                "authorization_servers": ["https://auth.example.test"]
            })));
            transport.enqueue(response(serde_json::json!({
                "issuer": "https://auth.example.test",
                "authorization_endpoint": "https://auth.example.test/authorize",
                "token_endpoint": "https://auth.example.test/token",
                "pushed_authorization_request_endpoint": "https://auth.example.test/par"
            })));
            transport.enqueue(response(serde_json::json!({
                "request_uri": request_uri,
                "expires_in": 60
            })));
        }

        let (mut first, _first_cancel) = operation(transport.clone());
        let (mut second, _second_cancel) = operation(transport.clone());
        first.start_authorization().await.unwrap();
        second.start_authorization().await.unwrap();

        let requests = transport.requests();
        let first_par = &requests[2];
        let second_par = &requests[5];
        assert_ne!(
            form_value(first_par, "state"),
            form_value(second_par, "state")
        );
        assert_ne!(
            form_value(first_par, "code_challenge"),
            form_value(second_par, "code_challenge")
        );
        assert_ne!(dpop_jwk(first_par), dpop_jwk(second_par));
    }

    #[tokio::test]
    async fn authorization_refuses_discovery_metadata_for_another_issuer_or_resource() {
        let transport = MockTransport::new();
        transport.enqueue(response(serde_json::json!({
            "resource": "https://other-pds.example.test",
            "authorization_servers": ["https://auth.example.test"]
        })));
        transport.enqueue(response(serde_json::json!({
            "issuer": "https://different-auth.example.test",
            "authorization_endpoint": "https://auth.example.test/authorize",
            "token_endpoint": "https://auth.example.test/token"
        })));
        let (mut operation, _cancel) = operation(transport.clone());
        let error = operation.start_authorization().await.unwrap_err();
        assert!(error.to_string().contains("does not name this account PDS"));
        assert_eq!(transport.requests().len(), 2);
    }

    #[tokio::test]
    async fn cancellation_wakes_a_suspended_owner_confirmation_wait() {
        let transport = MockTransport::new();
        let (mut operation, cancel) = operation(transport);
        operation.sleep = Rc::new(|_| Box::pin(std::future::pending()));
        let mut wait = Box::pin(operation.await_owner_confirmation(std::future::pending()));
        let waker = futures_util::task::noop_waker_ref();
        let mut context = std::task::Context::from_waker(waker);
        assert!(matches!(
            wait.as_mut().poll(&mut context),
            std::task::Poll::Pending
        ));

        cancel.cancel();
        let error = match wait.await {
            Ok(_) => panic!("cancellation must not yield owner input"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("canceled"));
    }

    #[tokio::test]
    async fn owner_confirmation_wait_has_a_finite_deadline_without_input() {
        let transport = MockTransport::new();
        let (mut operation, _cancel) = operation(transport);
        let error = match operation
            .await_owner_confirmation(std::future::pending())
            .await
        {
            Ok(_) => panic!("timer must not yield owner input"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn callback_state_and_issuer_refuse_before_token_exchange() {
        let transport = MockTransport::new();
        let (mut operation, _cancel) = operation(transport.clone());
        operation.authorization_server = Some(AuthorizationServerMetadata {
            issuer: "https://auth.example.test".into(),
            authorization_endpoint: "".into(),
            token_endpoint: "https://auth.example.test/token".into(),
            revocation_endpoint: None,
            pushed_authorization_request_endpoint: None,
            scopes_supported: vec![],
            response_types_supported: vec![],
            grant_types_supported: vec![],
            code_challenge_methods_supported: vec![],
            dpop_signing_alg_values_supported: vec![],
            token_endpoint_auth_methods_supported: vec![],
            require_pushed_authorization_requests: false,
        });
        let error = operation
            .complete(
                IdentityCallback::new(
                    "code".into(),
                    "wrong".into(),
                    "https://auth.example.test".into(),
                ),
                async { Ok(OwnerConfirmation::new("confirm".into())) },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("callback"));
        assert!(transport.requests().is_empty());
    }

    #[tokio::test]
    async fn late_token_after_cancellation_is_revoked_without_reviving_the_operation() {
        let inner = MockTransport::new();
        inner.enqueue(token("did:plc:alice"));
        inner.enqueue(status_response(200));
        inner.enqueue(status_response(200));
        let transport = CancelOnTokenResponseTransport::new(inner.clone());
        let mut rng = crate::crypto::OsRng;
        let (mut operation, cancellation) = IdentityOperation::new(
            transport.clone(),
            IdentityOperationConfig {
                pds_url: "https://pds.example.test".into(),
                did: "did:plc:alice".into(),
                redirect_uri: "https://app.example.test/callback".into(),
                change: VerificationMethodChange::Remove,
                now_micros: now,
                sleep: immediate_sleep(),
            },
            &mut rng,
        );
        operation.authorization_server = Some(server(Some("https://auth.example.test/revoke")));
        transport.cancel_when_token_arrives(cancellation);

        let result = operation
            .complete(callback(&operation), async {
                Ok(OwnerConfirmation::new("confirmed".into()))
            })
            .await
            .unwrap();

        assert_eq!(result.mutation, IdentityMutation::Canceled);
        assert_eq!(result.cleanup, IdentityCleanup::Attempted);
        let requests = inner.requests();
        assert_eq!(requests.len(), 3);
        assert!(requests[0].url.ends_with("/token"));
        assert!(requests[1].url.ends_with("/revoke"));
        assert!(requests[2].url.ends_with("/revoke"));
        assert!(
            !requests
                .iter()
                .any(|request| request.url.contains("requestPlcOperationSignature")),
            "a late token after cancellation must not start signing"
        );
    }

    #[tokio::test]
    async fn complete_is_one_shot_even_after_token_rejection() {
        let transport = MockTransport::new();
        transport.enqueue(response(serde_json::json!({"access_token":"access","refresh_token":"refresh","token_type":"DPoP","scope":"atproto identity:*","sub":"did:plc:other"})));
        transport.enqueue(status_response(200));
        transport.enqueue(status_response(200));
        let (mut operation, _cancel) = ready_operation(transport);
        let state = operation.state.to_string();
        let result = operation
            .complete(
                IdentityCallback::new(
                    "code".into(),
                    state.clone(),
                    "https://auth.example.test".into(),
                ),
                async { Ok(OwnerConfirmation::new("confirm".into())) },
            )
            .await
            .unwrap();
        assert_eq!(
            result.mutation,
            IdentityMutation::Refused {
                reason: IdentityRefusal::GrantRejected
            }
        );
        assert_eq!(result.cleanup, IdentityCleanup::Attempted);
        let error = operation
            .complete(
                IdentityCallback::new("code2".into(), state, "https://auth.example.test".into()),
                async { Ok(OwnerConfirmation::new("confirm".into())) },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("terminal"));
    }

    #[tokio::test]
    async fn successful_publication_keeps_the_complete_plc_map_and_scrubs_the_holder() {
        let transport = MockTransport::new();
        transport.enqueue(token("did:plc:alice"));
        transport.enqueue(status_response(200));
        transport.enqueue(did_document_without_opake());
        transport.enqueue(plc_state());
        transport.enqueue(response(
            serde_json::json!({"operation": {"type": "plc_operation"}}),
        ));
        transport.enqueue(status_response(200));
        transport.enqueue(status_response(200));
        transport.enqueue(status_response(200));
        let mut rng = crate::crypto::OsRng;
        let (mut operation, _cancel) = IdentityOperation::new(
            transport.clone(),
            IdentityOperationConfig {
                pds_url: "https://pds.example.test".into(),
                did: "did:plc:alice".into(),
                redirect_uri: "https://app.example.test/callback".into(),
                change: VerificationMethodChange::Publish([9; 32]),
                now_micros: now,
                sleep: immediate_sleep(),
            },
            &mut rng,
        );
        operation.authorization_server = Some(server(Some("https://auth.example.test/revoke")));

        let result = operation
            .complete(callback(&operation), async {
                Ok(OwnerConfirmation::new("confirmed".into()))
            })
            .await
            .unwrap();

        assert_eq!(result.mutation, IdentityMutation::Submitted);
        assert_eq!(result.reconciliation, None);
        assert_eq!(result.cleanup, IdentityCleanup::Attempted);
        let requests = transport.requests();
        assert_eq!(requests.len(), 8);
        assert!(requests[1].url.ends_with("requestPlcOperationSignature"));
        assert!(
            requests[1].body.is_none(),
            "confirmation request is bodyless"
        );
        assert!(requests[4].url.ends_with("signPlcOperation"));
        let Some(RequestBody::Json(sign_body)) = &requests[4].body else {
            panic!("sign request must carry the complete PLC operation body");
        };
        let methods = sign_body["verificationMethods"].as_object().unwrap();
        assert_eq!(methods["atproto"], "did:key:zexisting");
        assert_eq!(methods["unrelated"], "did:key:zunrelated");
        assert!(methods.contains_key("opake"));
        assert!(requests[5].url.ends_with("submitPlcOperation"));
        assert!(requests[6].url.ends_with("/revoke"));
        assert!(requests[7].url.ends_with("/revoke"));

        // Completion does not leave pending state, PKCE, state, or a usable
        // private DPoP key in an opaque holder retained for result inspection.
        assert!(operation.pkce.verifier.is_empty());
        assert!(operation.state.is_empty());
        assert!(operation.dpop_nonce.is_none());
        assert!(crate::client::dpop::create_dpop_proof(
            &operation.dpop_key,
            "POST",
            "https://auth.example.test/revoke",
            1,
            None,
            None,
            &mut crate::crypto::OsRng,
        )
        .is_err());
    }

    #[tokio::test]
    async fn wrong_subject_revokes_every_token_when_one_revocation_fails() {
        let transport = MockTransport::new();
        transport.enqueue(token("did:plc:other"));
        transport.enqueue(status_response(500));
        transport.enqueue(status_response(200));
        let (mut operation, _cancel) = ready_operation(transport.clone());

        let result = operation
            .complete(callback(&operation), async {
                Ok(OwnerConfirmation::new("confirmed".into()))
            })
            .await
            .unwrap();

        assert_eq!(
            result.mutation,
            IdentityMutation::Refused {
                reason: IdentityRefusal::GrantRejected
            }
        );
        assert_eq!(result.cleanup, IdentityCleanup::Failed);
        let requests = transport.requests();
        assert_eq!(requests.len(), 3);
        assert!(requests[1].url.ends_with("/revoke"));
        assert!(requests[2].url.ends_with("/revoke"));
    }

    #[tokio::test]
    async fn signer_refusal_reports_refusal_but_still_cleans_up() {
        let transport = MockTransport::new();
        transport.enqueue(token("did:plc:alice"));
        transport.enqueue(status_response(200));
        transport.enqueue(status_response(200));
        transport.enqueue(status_response(200));
        let (mut operation, _cancel) = ready_operation(transport.clone());

        let result = operation
            .complete(callback(&operation), async {
                Err(OwnerConfirmationFailure::Refused)
            })
            .await
            .unwrap();

        assert_eq!(
            result.mutation,
            IdentityMutation::Refused {
                reason: IdentityRefusal::ConfirmationRefused
            }
        );
        assert_eq!(result.cleanup, IdentityCleanup::Attempted);
        let requests = transport.requests();
        assert_eq!(requests.len(), 4);
        assert!(requests[1].url.ends_with("requestPlcOperationSignature"));
        assert!(requests[2].url.ends_with("/revoke"));
        assert!(requests[3].url.ends_with("/revoke"));
    }

    #[tokio::test]
    async fn confirmation_outcomes_preserve_refusal_delivery_failure_and_unknown() {
        let cases = [
            (
                OwnerConfirmationFailure::Refused,
                IdentityRefusal::ConfirmationRefused,
            ),
            (
                OwnerConfirmationFailure::DeliveryFailed,
                IdentityRefusal::ConfirmationDeliveryFailed,
            ),
            (
                OwnerConfirmationFailure::DeliveryUnknown,
                IdentityRefusal::ConfirmationDeliveryUnknown,
            ),
        ];
        for (failure, expected_reason) in cases {
            let transport = MockTransport::new();
            transport.enqueue(token("did:plc:alice"));
            transport.enqueue(status_response(200));
            transport.enqueue(status_response(200));
            transport.enqueue(status_response(200));
            let (mut operation, _cancel) = ready_operation(transport.clone());

            let result = operation
                .complete(callback(&operation), async move { Err(failure) })
                .await
                .unwrap();

            assert_eq!(
                result.mutation,
                IdentityMutation::Refused {
                    reason: expected_reason
                }
            );
            assert_eq!(result.cleanup, IdentityCleanup::Attempted);
            assert_eq!(transport.requests().len(), 4);
        }
    }

    #[tokio::test]
    async fn rejected_confirmation_request_is_not_reported_as_delivery_failure() {
        let transport = MockTransport::new();
        transport.enqueue(token("did:plc:alice"));
        transport.enqueue(status_response(403));
        transport.enqueue(status_response(200));
        transport.enqueue(status_response(200));
        let (mut operation, _cancel) = ready_operation(transport.clone());

        let result = operation
            .complete(callback(&operation), async {
                Ok(OwnerConfirmation::new("unreachable".into()))
            })
            .await
            .unwrap();

        assert_eq!(
            result.mutation,
            IdentityMutation::Refused {
                reason: IdentityRefusal::ConfirmationRequestRefused
            }
        );
        assert_eq!(result.cleanup, IdentityCleanup::Attempted);
        assert_eq!(transport.requests().len(), 4);
    }

    #[tokio::test]
    async fn cancellation_cleans_up_after_a_bounded_revocation_timeout() {
        let inner = MockTransport::new();
        inner.enqueue(token("did:plc:alice"));
        inner.enqueue(status_response(200));
        inner.enqueue(status_response(200));
        inner.enqueue(status_response(200));
        let transport = TimeoutFirstRevocationTransport::new(inner.clone());
        let mut rng = crate::crypto::OsRng;
        let (mut operation, cancel) = IdentityOperation::new(
            transport,
            IdentityOperationConfig {
                pds_url: "https://pds.example.test".into(),
                did: "did:plc:alice".into(),
                redirect_uri: "https://app.example.test/callback".into(),
                change: VerificationMethodChange::Remove,
                now_micros: now,
                sleep: immediate_sleep(),
            },
            &mut rng,
        );
        operation.authorization_server = Some(server(Some("https://auth.example.test/revoke")));
        let callback = IdentityCallback::new(
            "code".into(),
            operation.state.to_string(),
            "https://auth.example.test".into(),
        );

        let result = operation
            .complete(callback, async move {
                cancel.cancel();
                Err(OwnerConfirmationFailure::Refused)
            })
            .await
            .unwrap();

        assert_eq!(result.mutation, IdentityMutation::Canceled);
        assert_eq!(result.cleanup, IdentityCleanup::Failed);
        let requests = inner.requests();
        assert_eq!(requests.len(), 4);
        assert!(requests[2].url.ends_with("/revoke"));
        assert!(requests[3].url.ends_with("/revoke"));
    }

    #[tokio::test]
    async fn lost_submission_response_reports_unknown_with_every_fresh_did_observation() {
        let cases = vec![
            (
                did_document_with_opake([9; 32]),
                IdentityReconciliation::ObservedMatching,
            ),
            (
                did_document_without_opake(),
                IdentityReconciliation::ObservedUnchanged,
            ),
            (
                did_document_with_opake([8; 32]),
                IdentityReconciliation::ObservedConflict,
            ),
            (status_response(503), IdentityReconciliation::Unavailable),
        ];

        for (reconciliation_response, expected_observation) in cases {
            let transport = MockTransport::new();
            transport.enqueue(token("did:plc:alice"));
            transport.enqueue(status_response(200));
            transport.enqueue(did_document_without_opake());
            transport.enqueue(plc_state());
            transport.enqueue(response(
                serde_json::json!({"operation": {"type": "plc_operation"}}),
            ));
            // The submission reached the transport but did not yield a
            // usable answer, so only a new DID read is safe to report.
            transport.enqueue(status_response(503));
            transport.enqueue(reconciliation_response);
            transport.enqueue(status_response(200));
            transport.enqueue(status_response(200));
            let mut rng = crate::crypto::OsRng;
            let (mut operation, _cancel) = IdentityOperation::new(
                transport.clone(),
                IdentityOperationConfig {
                    pds_url: "https://pds.example.test".into(),
                    did: "did:plc:alice".into(),
                    redirect_uri: "https://app.example.test/callback".into(),
                    change: VerificationMethodChange::Publish([9; 32]),
                    now_micros: now,
                    sleep: immediate_sleep(),
                },
                &mut rng,
            );
            operation.authorization_server = Some(server(Some("https://auth.example.test/revoke")));

            let result = operation
                .complete(callback(&operation), async {
                    Ok(OwnerConfirmation::new("confirmed".into()))
                })
                .await
                .unwrap();

            assert_eq!(result.mutation, IdentityMutation::Unknown);
            assert_eq!(result.reconciliation, Some(expected_observation));
            assert_eq!(result.cleanup, IdentityCleanup::Attempted);
            assert_eq!(transport.requests().len(), 9);
        }
    }

    #[tokio::test]
    async fn removal_reconciliation_compares_the_pre_submit_key() {
        let existing =
            crate::storage::Identity::generate("did:plc:alice", &mut crate::crypto::OsRng)
                .verify_key_bytes()
                .unwrap()
                .unwrap();
        let foreign =
            crate::storage::Identity::generate("did:plc:other", &mut crate::crypto::OsRng)
                .verify_key_bytes()
                .unwrap()
                .unwrap();
        let cases = vec![
            (
                did_document_without_opake(),
                IdentityReconciliation::ObservedMatching,
            ),
            (
                did_document_with_opake(existing),
                IdentityReconciliation::ObservedUnchanged,
            ),
            (
                did_document_with_opake(foreign),
                IdentityReconciliation::ObservedConflict,
            ),
        ];

        for (reconciliation_response, expected_observation) in cases {
            let transport = MockTransport::new();
            transport.enqueue(token("did:plc:alice"));
            transport.enqueue(status_response(200));
            transport.enqueue(did_document_with_opake(existing));
            transport.enqueue(plc_state_with_opake(existing));
            transport.enqueue(response(
                serde_json::json!({"operation": {"type": "plc_operation"}}),
            ));
            transport.enqueue(status_response(503));
            transport.enqueue(reconciliation_response);
            transport.enqueue(status_response(200));
            transport.enqueue(status_response(200));
            let (mut operation, _cancel) = ready_operation(transport);

            let result = operation
                .complete(callback(&operation), async {
                    Ok(OwnerConfirmation::new("confirmed".into()))
                })
                .await
                .unwrap();

            assert_eq!(result.mutation, IdentityMutation::Unknown);
            assert_eq!(result.reconciliation, Some(expected_observation));
        }
    }

    #[test]
    fn signing_request_preserves_unrelated_plc_verification_methods() {
        let transport = MockTransport::new();
        let mut rng = crate::crypto::OsRng;
        let (operation, _cancel) = IdentityOperation::new(
            transport,
            IdentityOperationConfig {
                pds_url: "https://pds.example.test".into(),
                did: "did:plc:alice".into(),
                redirect_uri: "https://app.example.test/callback".into(),
                change: VerificationMethodChange::Publish([7; 32]),
                now_micros: now,
                sleep: immediate_sleep(),
            },
            &mut rng,
        );
        let body = operation.sign_request_body(serde_json::json!({
            "rotationKeys": ["did:key:zrotation"],
            "alsoKnownAs": ["at://alice.test"],
            "verificationMethods": {"atproto": "did:key:zexisting", "other": "did:key:zother"},
            "services": {"atproto_pds": {"type": "AtprotoPersonalDataServer", "endpoint": "https://pds.example.test"}}
        }), "confirmation").unwrap();
        let methods = body.get("verificationMethods").unwrap();
        assert_eq!(methods.get("atproto").unwrap(), "did:key:zexisting");
        assert_eq!(methods.get("other").unwrap(), "did:key:zother");
        assert!(methods.get("opake").is_some());
    }
}
