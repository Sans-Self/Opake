mod service;

use std::time::Duration;

use anyhow::Result;
use clap::{Args, Subcommand};
use log::{error, info, warn};
use opake_core::client::session_refresh::{
    proactive_refresh, RefreshOutcome, DEFAULT_REFRESH_THRESHOLD_SECONDS,
};
use opake_core::client::{time, ReqwestTransport, Session};
use opake_core::crypto::OsRng;

use crate::config::FileStorage;
use crate::session;

/// Background daemon for proactive session maintenance
#[derive(Args)]
pub struct DaemonCommand {
    #[command(subcommand)]
    action: DaemonAction,
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Run the daemon in the foreground. Checks all accounts periodically.
    Run(RunArgs),
    /// Generate and install a system service file (launchd or systemd)
    Install(InstallArgs),
    /// Remove the installed system service file
    Uninstall(UninstallArgs),
}

#[derive(Args)]
struct RunArgs {
    /// Check interval in seconds
    #[arg(long, default_value_t = 60)]
    interval: u64,
    /// Refresh threshold in seconds (refresh if expiring within this window)
    #[arg(long, default_value_t = DEFAULT_REFRESH_THRESHOLD_SECONDS)]
    threshold: i64,
}

#[derive(Args)]
struct InstallArgs;

#[derive(Args)]
struct UninstallArgs;

impl DaemonCommand {
    pub async fn execute(self, storage: &FileStorage) -> Result<()> {
        match self.action {
            DaemonAction::Run(args) => run_daemon(storage, args).await,
            DaemonAction::Install(_) => service::install(storage),
            DaemonAction::Uninstall(_) => service::uninstall(),
        }
    }
}

// ---------------------------------------------------------------------------
// Daemon loop
// ---------------------------------------------------------------------------

async fn run_daemon(storage: &FileStorage, args: RunArgs) -> Result<()> {
    println!(
        "opake daemon starting (interval={}s, threshold={}s)",
        args.interval, args.threshold
    );

    let transport = ReqwestTransport::new();
    let mut interval = tokio::time::interval(Duration::from_secs(args.interval));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                refresh_all_accounts(storage, &transport, args.threshold).await;
            }
            _ = tokio::signal::ctrl_c() => {
                info!("received SIGINT, shutting down");
                println!("shutting down");
                break;
            }
        }
    }

    Ok(())
}

/// Refresh all accounts' sessions. Errors on individual accounts are logged,
/// not fatal. This is extracted as a standalone function for testability.
async fn refresh_all_accounts(storage: &FileStorage, transport: &ReqwestTransport, threshold: i64) {
    let config = match storage.load_config_anyhow() {
        Ok(c) => c,
        Err(e) => {
            warn!("daemon: failed to load config: {e}");
            return;
        }
    };

    let now = time::unix_now();

    for (did, account) in &config.accounts {
        let session: Session = match storage.load_account_json(did, "session.json") {
            Ok(s) => s,
            Err(e) => {
                warn!("daemon: failed to load session for {did}: {e}");
                continue;
            }
        };

        let result = proactive_refresh(
            transport,
            &session,
            &account.pds_url,
            threshold,
            now,
            &mut OsRng,
        )
        .await;

        match result {
            RefreshOutcome::Refreshed(new_session) => {
                if let Err(e) = session::persist_session(storage, did, &new_session) {
                    error!("daemon: failed to persist refreshed session for {did}: {e}");
                } else {
                    info!("daemon: refreshed session for {}", new_session.handle());
                }
            }
            RefreshOutcome::NotNeeded => {
                info!("daemon: session for {} still valid", session.handle());
            }
            RefreshOutcome::Failed(e) => {
                warn!("daemon: refresh failed for {}: {e}", session.handle());
            }
        }
    }
}
