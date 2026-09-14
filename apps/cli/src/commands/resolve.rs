use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use clap::Args;
use opake_core::client::Session;
use opake_core::resolve::{self, AnchorHistory};

use crate::commands::Execute;
use crate::session::CommandContext;
use opake_core::client::ReqwestTransport;

#[derive(Args)]
/// Resolve a user's DID and encryption public key
pub struct ResolveCommand {
    /// Handle or DID to resolve
    target: String,
}

impl Execute for ResolveCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let transport = ReqwestTransport::new();
        let identity = resolve::resolve_identity(&transport, &ctx.pds_url, &self.target).await?;

        if let Some(ref handle) = identity.handle {
            println!("{}", handle);
        }
        println!("  DID:        {}", identity.did);
        println!("  PDS:        {}", identity.pds_url);
        println!(
            "  X25519:     {}",
            BASE64.encode(identity.x25519_public_key)
        );
        println!("  X25519 algo: {}", identity.x25519_algo);
        println!(
            "  ML-KEM-768: {}…  ({} bytes)",
            BASE64.encode(&identity.ml_kem_public_key[..32]),
            identity.ml_kem_public_key.len()
        );
        println!("  ML-KEM algo: {}", identity.ml_kem_algo);
        for line in verification_status_lines(&identity.verification) {
            println!("  {line}");
        }

        Ok(None)
    }
}

fn verification_status_lines(verification: &resolve::VerificationState) -> [&'static str; 2] {
    match verification {
        resolve::VerificationState::Unverified => [
            "Verification: Unverified",
            "Verification method history: no verification method is published",
        ],
        resolve::VerificationState::Verified {
            anchor_history: AnchorHistory::Replaced,
        } => [
            "Verification: Verified",
            "Verification method history: the verification key was replaced",
        ],
        resolve::VerificationState::Verified {
            anchor_history: AnchorHistory::NotReplaced,
        } => [
            "Verification: Verified",
            "Verification method history: no replacement was found",
        ],
        resolve::VerificationState::Verified {
            anchor_history: AnchorHistory::NoHistory,
        } => [
            "Verification: Verified",
            "Verification method history: this DID method publishes none",
        ],
        resolve::VerificationState::Verified {
            anchor_history: AnchorHistory::Unavailable,
        } => [
            "Verification: Verified",
            "Verification method history: unavailable; replacement is unknown",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_absent_unreadable_and_unpublished_history_distinctly() {
        assert_eq!(
            verification_status_lines(&resolve::VerificationState::Unverified),
            [
                "Verification: Unverified",
                "Verification method history: no verification method is published",
            ]
        );
        assert_eq!(
            verification_status_lines(&resolve::VerificationState::Verified {
                anchor_history: AnchorHistory::NoHistory
            }),
            [
                "Verification: Verified",
                "Verification method history: this DID method publishes none",
            ]
        );
        assert_eq!(
            verification_status_lines(&resolve::VerificationState::Verified {
                anchor_history: AnchorHistory::Unavailable
            }),
            [
                "Verification: Verified",
                "Verification method history: unavailable; replacement is unknown",
            ]
        );
    }
}
