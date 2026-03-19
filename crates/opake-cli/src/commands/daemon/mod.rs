mod service;

use std::time::Duration;

use anyhow::Result;
use clap::{Args, Subcommand};
use log::{error, info, warn};
use opake_core::client::session_refresh::{proactive_refresh, RefreshOutcome};
use opake_core::client::{time, ReqwestTransport, Session};
use opake_core::crypto::OsRng;
use opake_core::daemon::{self, TASKS};
use opake_core::pairing::cleanup_expired_pair_requests;
use opake_core::sharing::heal_stale_grants;

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
    /// Run the daemon in the foreground. Runs maintenance tasks on independent schedules.
    Run(RunArgs),
    /// Generate and install a system service file (launchd or systemd)
    Install(InstallArgs),
    /// Remove the installed system service file
    Uninstall(UninstallArgs),
}

#[derive(Args)]
struct RunArgs {
    /// Session refresh threshold in seconds (refresh if expiring within this window)
    #[arg(long, default_value_t = daemon::SESSION_REFRESH_THRESHOLD)]
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
// Daemon loop — intervals derived from core task registry
// ---------------------------------------------------------------------------

fn task_interval(name: &str) -> Duration {
    let task = daemon::task_by_name(name).expect("unknown daemon task");
    Duration::from_secs(task.interval_seconds as u64)
}

async fn run_daemon(storage: &FileStorage, args: RunArgs) -> Result<()> {
    let task_summary: String = TASKS
        .iter()
        .map(|t| format!("{}={}s", t.name, t.interval_seconds))
        .collect::<Vec<_>>()
        .join(", ");
    println!("opake daemon starting ({task_summary})");

    let transport = ReqwestTransport::new();

    let mut session_tick = tokio::time::interval(task_interval("session-refresh"));
    let mut pair_tick = tokio::time::interval(task_interval("pair-cleanup"));
    let mut grant_tick = tokio::time::interval(task_interval("grant-healing"));

    loop {
        tokio::select! {
            _ = session_tick.tick() => {
                run_session_refresh(storage, &transport, args.threshold).await;
            }
            _ = pair_tick.tick() => {
                run_pair_cleanup(storage, &transport).await;
            }
            _ = grant_tick.tick() => {
                run_grant_healing(storage, &transport).await;
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

// ---------------------------------------------------------------------------
// Task: session refresh
// ---------------------------------------------------------------------------

async fn run_session_refresh(storage: &FileStorage, transport: &ReqwestTransport, threshold: i64) {
    let Some(config) = load_config_or_warn(storage, "session-refresh") else {
        return;
    };

    let now = time::unix_now();

    for (did, account) in &config.accounts {
        let session: Session = match storage.load_account_json(did, "session.json") {
            Ok(s) => s,
            Err(e) => {
                warn!("session-refresh: failed to load session for {did}: {e}");
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
                    error!("session-refresh: failed to persist for {did}: {e}");
                } else {
                    info!("session-refresh: refreshed for {}", new_session.handle());
                }
            }
            RefreshOutcome::NotNeeded => {}
            RefreshOutcome::Failed(e) => {
                warn!("session-refresh: failed for {did}: {e}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Task: pair request cleanup
// ---------------------------------------------------------------------------

async fn run_pair_cleanup(storage: &FileStorage, transport: &ReqwestTransport) {
    let Some(config) = load_config_or_warn(storage, "pair-cleanup") else {
        return;
    };

    let now = time::unix_now();

    for (did, account) in &config.accounts {
        let mut client = match build_client(storage, transport, did, &account.pds_url) {
            Ok(c) => c,
            Err(e) => {
                warn!("pair-cleanup: failed to build client for {did}: {e}");
                continue;
            }
        };

        if let Err(e) = cleanup_expired_pair_requests(
            &mut client,
            now,
            opake_core::pairing::DEFAULT_PAIR_REQUEST_TTL_SECONDS,
        )
        .await
        {
            warn!("pair-cleanup: failed for {did}: {e}");
        }

        persist_if_refreshed(storage, did, &client);
    }
}

// ---------------------------------------------------------------------------
// Task: grant healing
// ---------------------------------------------------------------------------

async fn run_grant_healing(storage: &FileStorage, transport: &ReqwestTransport) {
    let Some(config) = load_config_or_warn(storage, "grant-healing") else {
        return;
    };

    for (did, account) in &config.accounts {
        let mut client = match build_client(storage, transport, did, &account.pds_url) {
            Ok(c) => c,
            Err(e) => {
                warn!("grant-heal: failed to build client for {did}: {e}");
                continue;
            }
        };

        if let Err(e) = heal_stale_grants(&mut client).await {
            warn!("grant-heal: failed for {did}: {e}");
        }

        persist_if_refreshed(storage, did, &client);
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn load_config_or_warn(storage: &FileStorage, task: &str) -> Option<crate::config::Config> {
    match storage.load_config_anyhow() {
        Ok(c) => Some(c),
        Err(e) => {
            warn!("{task}: failed to load config: {e}");
            None
        }
    }
}

fn build_client(
    storage: &FileStorage,
    transport: &ReqwestTransport,
    did: &str,
    pds_url: &str,
) -> Result<opake_core::client::XrpcClient<ReqwestTransport>> {
    let session: Session = storage.load_account_json(did, "session.json")?;
    Ok(opake_core::client::XrpcClient::with_session(
        transport.clone(),
        pds_url.to_string(),
        session,
    ))
}

fn persist_if_refreshed(
    storage: &FileStorage,
    did: &str,
    client: &opake_core::client::XrpcClient<ReqwestTransport>,
) {
    if client.session_refreshed() {
        if let Some(s) = client.session() {
            if let Err(e) = session::persist_session(storage, did, s) {
                error!("daemon: failed to persist session for {did}: {e}");
            }
        }
    }
}
