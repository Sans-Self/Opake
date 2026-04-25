use anyhow::Result;
use clap::{Args, Subcommand};
use log::info;
use opake_core::client::session_refresh::{
    proactive_refresh, RefreshOutcome, DEFAULT_REFRESH_THRESHOLD_SECONDS,
};
use opake_core::client::{time, ReqwestTransport, Session};
use opake_core::crypto::OsRng;

use crate::config::FileStorage;
use crate::session;

/// Manage session tokens
#[derive(Args)]
pub struct SessionCommand {
    #[command(subcommand)]
    action: SessionAction,
}

#[derive(Subcommand)]
enum SessionAction {
    /// Refresh the access token if it's expiring soon
    Refresh(RefreshArgs),
}

#[derive(Args)]
struct RefreshArgs {
    /// Seconds before expiry to trigger refresh (default: 300)
    #[arg(long, default_value_t = DEFAULT_REFRESH_THRESHOLD_SECONDS)]
    threshold: i64,
}

impl SessionCommand {
    pub async fn execute(self, storage: &FileStorage) -> Result<Option<Session>> {
        match self.action {
            SessionAction::Refresh(args) => refresh(storage, args.threshold).await,
        }
    }
}

async fn refresh(storage: &FileStorage, threshold: i64) -> Result<Option<Session>> {
    let ctx = session::resolve_context(storage, None)?;
    let session: Session = ctx.storage.load_account_json(&ctx.did, "session.json")?;
    let transport = ReqwestTransport::new();
    let now = time::unix_now();

    let result = proactive_refresh(
        &transport,
        &session,
        &ctx.pds_url,
        threshold,
        now,
        &mut OsRng,
    )
    .await;

    match result {
        RefreshOutcome::Refreshed(new_session) => {
            session::persist_session(&ctx.storage, &ctx.did, &new_session)?;
            let remaining = new_session
                .expires_at()
                .map(|e| (e - now) / 60)
                .unwrap_or(0);
            info!("session refreshed for {}", new_session.handle());
            println!(
                "Session refreshed for {} (valid for ~{} min)",
                new_session.handle(),
                remaining
            );
            Ok(Some(*new_session))
        }
        RefreshOutcome::NotNeeded => {
            let remaining = session.expires_at().map(|e| (e - now) / 60).unwrap_or(0);
            println!(
                "Session still valid for {} (~{} min remaining)",
                session.handle(),
                remaining
            );
            Ok(None)
        }
        RefreshOutcome::Failed(e) => {
            anyhow::bail!("session refresh failed for {}: {e}", session.handle());
        }
    }
}
