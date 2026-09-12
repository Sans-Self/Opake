use std::io::{self, Write};
use std::thread;

use anyhow::Result;
use clap::{Args, Subcommand};
use opake_core::client::identity_operation::{
    IdentityCallback, IdentityCleanup, IdentityMutation, IdentityOperationCancellation,
    IdentityOperationResult, IdentityReconciliation, IdentityRefusal, OwnerConfirmation,
    OwnerConfirmationFailure,
};
use opake_core::resolve::SelfVerificationState;
use tokio::sync::oneshot;
use zeroize::{Zeroize, Zeroizing};

use crate::commands::Execute;
use crate::oauth::{bind_loopback_callback, open_browser, wait_for_callback};
use crate::session::CommandContext;

/// Manage this account's `#opake` DID verification method.
#[derive(Args)]
pub struct VerificationCommand {
    #[command(subcommand)]
    pub(crate) action: VerificationAction,
}

#[derive(Subcommand)]
pub enum VerificationAction {
    /// Publish this device's signing key as the account's verification method
    Setup,
    /// Remove the account's verification method
    Remove,
    /// Check the verification method currently published in the DID document
    Status,
}

impl Execute for VerificationCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<opake_core::client::Session>> {
        match self.action {
            VerificationAction::Setup => run_identity_operation(ctx, IdentityAction::Setup).await?,
            VerificationAction::Remove => {
                run_identity_operation(ctx, IdentityAction::Remove).await?
            }
            VerificationAction::Status => print_status(ctx).await?,
        }
        Ok(None)
    }
}

enum IdentityAction {
    Setup,
    Remove,
}

async fn print_status(ctx: &CommandContext) -> Result<()> {
    let opake = ctx.opake().await?;
    match opake.check_own_verification().await? {
        SelfVerificationState::Absent => println!("Verification method: absent"),
        SelfVerificationState::Verified => println!("Verification method: verified"),
        SelfVerificationState::Substitution => {
            println!(
                "Verification method: substitution (the published key is not this device's key)"
            )
        }
    }
    Ok(())
}

async fn run_identity_operation(ctx: &CommandContext, action: IdentityAction) -> Result<()> {
    // The listener is bound before PAR so the exact redirect URI is part of
    // this operation's PKCE/state binding. It is dropped on every exit.
    let (listener, redirect_uri) = bind_loopback_callback().await?;
    let mut opake = ctx.opake().await?;
    let (mut operation, cancellation) = match action {
        // This standing-session write intentionally precedes construction of
        // the PLC operation: once #opake exists, peers can verify the record.
        IdentityAction::Setup => {
            opake
                .prepare_verification_method_publication(redirect_uri)
                .await?
        }
        IdentityAction::Remove => opake.new_verification_method_removal(redirect_uri)?,
    };

    let authorization_url = tokio::select! {
        result = operation.start_authorization() => result?,
        _ = tokio::signal::ctrl_c() => {
            cancellation.cancel();
            anyhow::bail!("Verification operation cancelled")
        }
    };
    println!("Open this URL to authorize the verification operation:");
    println!("  {authorization_url}");
    println!(
        "This uses a separate authorization for this operation. Your provider may reuse prior approval; Opake does not require a new consent screen for every operation."
    );
    open_browser(&authorization_url);

    let callback = tokio::select! {
        result = wait_for_callback(listener, 10 * 60, None) => result?,
        _ = tokio::signal::ctrl_c() => {
            cancellation.cancel();
            anyhow::bail!("Verification operation cancelled")
        }
    };
    if let Some(error) = callback.error {
        cancellation.cancel();
        anyhow::bail!("authorization server refused the verification operation: {error}");
    }
    let code = callback
        .code
        .ok_or_else(|| anyhow::anyhow!("authorization callback missing code"))?;
    let state = callback
        .state
        .ok_or_else(|| anyhow::anyhow!("authorization callback missing state"))?;
    let issuer = callback
        .issuer
        .ok_or_else(|| anyhow::anyhow!("authorization callback missing issuer"))?;

    // `complete` makes the bodyless signer request before polling this future.
    // Therefore the terminal never asks for owner input until the signer has
    // accepted a request and is actually waiting for that input.
    let completion = operation.complete(
        IdentityCallback::new(code, state, issuer),
        read_owner_confirmation(cancellation.clone()),
    );
    tokio::pin!(completion);
    // Do not drop an in-flight completion on Ctrl-C: cancelling wakes core at
    // the next safe boundary, and awaiting it lets owned credentials reach
    // its mandatory cleanup path.
    let result = tokio::select! {
        result = &mut completion => result?,
        _ = tokio::signal::ctrl_c() => {
            cancellation.cancel();
            completion.await?
        }
    };
    print_result(&result);
    Ok(())
}

async fn read_owner_confirmation(
    cancellation: IdentityOperationCancellation,
) -> Result<OwnerConfirmation, OwnerConfirmationFailure> {
    println!("Your provider accepted the confirmation request.");
    print!("Enter the confirmation supplied by the account owner (or press Enter to refuse): ");
    io::stdout()
        .flush()
        .map_err(|_| OwnerConfirmationFailure::DeliveryUnknown)?;

    let (sender, receiver) = oneshot::channel();
    // A detached OS thread avoids Tokio waiting for an unread terminal at
    // shutdown. If cancellation/deadline drops the receiver, the late input
    // is zeroized by this thread instead of surviving in a task handle.
    thread::spawn(move || {
        let mut input = Zeroizing::new(String::new());
        let result = io::stdin().read_line(&mut input).map(|_| {
            let confirmation = input.trim().to_owned();
            input.zeroize();
            confirmation
        });
        if let Err(Ok(mut late_input)) = sender.send(result) {
            late_input.zeroize();
        }
    });
    wait_for_owner_input(receiver, cancellation, tokio::signal::ctrl_c()).await
}

async fn wait_for_owner_input<F>(
    receiver: oneshot::Receiver<io::Result<String>>,
    cancellation: IdentityOperationCancellation,
    cancel: F,
) -> Result<OwnerConfirmation, OwnerConfirmationFailure>
where
    F: std::future::Future<Output = Result<(), io::Error>>,
{
    tokio::select! {
        input = receiver => match input {
            Ok(Ok(value)) if !value.is_empty() => Ok(OwnerConfirmation::new(value)),
            Ok(Ok(_)) => Err(OwnerConfirmationFailure::Refused),
            Ok(Err(_)) | Err(_) => Err(OwnerConfirmationFailure::DeliveryUnknown),
        },
        _ = cancel => {
            cancellation.cancel();
            Err(OwnerConfirmationFailure::Refused)
        }
    }
}

fn print_result(result: &IdentityOperationResult) {
    match &result.mutation {
        IdentityMutation::Submitted => println!("Verification operation submitted."),
        IdentityMutation::Canceled => println!("Verification operation cancelled."),
        IdentityMutation::Unknown => println!(
            "Verification submission outcome is unknown; check `opake verification status` before trying again."
        ),
        IdentityMutation::Refused { reason } => println!("{}", refusal_message(*reason)),
    }
    match result.cleanup {
        IdentityCleanup::Attempted => {
            println!("Temporary identity authorization cleanup was attempted.")
        }
        IdentityCleanup::Failed => {
            println!("Temporary identity authorization cleanup encountered a failure.")
        }
        IdentityCleanup::Unavailable => {
            println!("No temporary-authorization cleanup endpoint was available.")
        }
    }
    match result.reconciliation {
        Some(IdentityReconciliation::ObservedMatching) => {
            println!("A fresh DID read now matches the requested verification state.")
        }
        Some(IdentityReconciliation::ObservedUnchanged) => {
            println!("A fresh DID read still has the previous verification state.")
        }
        Some(IdentityReconciliation::ObservedConflict) => {
            println!("A fresh DID read changed differently; do not retry until it is resolved.")
        }
        Some(IdentityReconciliation::Unavailable) => {
            println!("A fresh DID read was unavailable, so the submission remains uncertain.")
        }
        None => {}
    }
}

fn refusal_message(reason: IdentityRefusal) -> &'static str {
    match reason {
        IdentityRefusal::GrantRejected => "The authorization grant was rejected.",
        IdentityRefusal::ConfirmationRequestRefused => {
            "The provider refused the confirmation request."
        }
        IdentityRefusal::ConfirmationRefused => "The owner refused confirmation.",
        IdentityRefusal::ConfirmationDeliveryFailed => {
            "The provider reported that confirmation delivery failed."
        }
        IdentityRefusal::ConfirmationDeliveryUnknown => {
            "Confirmation delivery could not be determined."
        }
        IdentityRefusal::PreparationFailed => "The verification operation could not be prepared.",
        IdentityRefusal::SignerRefused => {
            "The provider refused to sign the verification operation."
        }
        IdentityRefusal::SignerResponseUnknown => {
            "The provider's signing response could not be determined."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn owner_input_is_not_consumed_after_cancellation() {
        let (_, cancellation) = opake_core::client::identity_operation::IdentityOperation::<
            opake_core::client::ReqwestTransport,
        >::new(
            opake_core::client::ReqwestTransport::new(),
            opake_core::client::identity_operation::IdentityOperationConfig {
                pds_url: "https://pds.test".into(),
                did: "did:plc:test".into(),
                redirect_uri: "http://127.0.0.1/callback".into(),
                change: opake_core::client::identity_operation::VerificationMethodChange::Remove,
                now_micros: || 0,
                sleep: std::rc::Rc::new(|_| Box::pin(std::future::pending())),
            },
            &mut opake_core::crypto::OsRng,
        );
        let (_sender, receiver) = oneshot::channel();
        let result = wait_for_owner_input(receiver, cancellation.clone(), async { Ok(()) }).await;
        assert!(matches!(result, Err(OwnerConfirmationFailure::Refused)));
        assert!(cancellation.is_cancelled());
    }

    #[test]
    fn delivery_unknown_is_not_described_as_refusal() {
        assert_eq!(
            refusal_message(IdentityRefusal::ConfirmationDeliveryUnknown),
            "Confirmation delivery could not be determined."
        );
    }
}
