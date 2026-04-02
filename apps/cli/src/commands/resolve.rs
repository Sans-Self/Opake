use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use clap::Args;
use opake_core::client::Session;
use opake_core::resolve;

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
        println!("  Public key: {}", BASE64.encode(identity.public_key));
        println!("  Algorithm:  {}", identity.algo);

        Ok(None)
    }
}
