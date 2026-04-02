mod service;

use std::time::Duration;

use anyhow::Result;
use clap::{Args, Subcommand};
use log::{info, warn};
use opake_core::client::ReqwestTransport;
use opake_core::crypto::OsRng;
use opake_core::daemon::{self, TASKS};
use opake_core::opake::Opake;

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
    /// List background tasks (re-encryption, etc.) and their status.
    ListTasks,
    /// Generate and install a system service file (launchd or systemd)
    Install(InstallArgs),
    /// Remove the installed system service file
    Uninstall(UninstallArgs),
}

#[derive(Args)]
struct RunArgs {}

#[derive(Args)]
struct InstallArgs;

#[derive(Args)]
struct UninstallArgs;

impl DaemonCommand {
    pub async fn execute(self, storage: &FileStorage) -> Result<()> {
        match self.action {
            DaemonAction::Run(args) => run_daemon(storage, args).await,
            DaemonAction::ListTasks => list_tasks(storage).await,
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

async fn run_daemon(storage: &FileStorage, _args: RunArgs) -> Result<()> {
    let task_summary: String = TASKS
        .iter()
        .map(|t| format!("{}={}s", t.name, t.interval_seconds))
        .collect::<Vec<_>>()
        .join(", ");
    println!("opake daemon starting ({task_summary})");

    let mut pair_tick = tokio::time::interval(task_interval("pair-cleanup"));
    let mut grant_tick = tokio::time::interval(task_interval("grant-healing"));
    let mut share_tick = tokio::time::interval(task_interval("share-retry"));
    let mut dir_sync_tick = tokio::time::interval(task_interval("directory-sync"));

    loop {
        tokio::select! {
            _ = pair_tick.tick() => {
                info!("running: pair-cleanup");
                run_pair_cleanup(storage).await;
            }
            _ = grant_tick.tick() => {
                info!("running: grant-healing");
                run_grant_healing(storage).await;
            }
            _ = share_tick.tick() => {
                info!("running: share-retry");
                run_share_retry(storage).await;
            }
            _ = dir_sync_tick.tick() => {
                info!("running: directory-sync");
                run_directory_sync(storage).await;
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
// Task: pair request cleanup
// ---------------------------------------------------------------------------

async fn run_pair_cleanup(storage: &FileStorage) {
    let Some(config) = load_config_or_warn(storage, "pair-cleanup") else {
        return;
    };

    for did in config.accounts.keys() {
        let mut opake = match build_opake(storage, did).await {
            Ok(o) => o,
            Err(e) => {
                warn!("pair-cleanup: failed to build opake for {did}: {e}");
                continue;
            }
        };

        if let Err(e) = opake
            .cleanup_expired_pair_requests(opake_core::pairing::DEFAULT_PAIR_REQUEST_TTL_SECONDS)
            .await
        {
            warn!("pair-cleanup: failed for {did}: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Task: grant healing
// ---------------------------------------------------------------------------

async fn run_grant_healing(storage: &FileStorage) {
    let Some(config) = load_config_or_warn(storage, "grant-healing") else {
        return;
    };

    for did in config.accounts.keys() {
        let mut opake = match build_opake(storage, did).await {
            Ok(o) => o,
            Err(e) => {
                warn!("grant-heal: failed to build opake for {did}: {e}");
                continue;
            }
        };

        if let Err(e) = opake.heal_stale_grants().await {
            warn!("grant-heal: failed for {did}: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Task: pending share retry
// ---------------------------------------------------------------------------

async fn run_share_retry(storage: &FileStorage) {
    let Some(config) = load_config_or_warn(storage, "share-retry") else {
        return;
    };

    for did in config.accounts.keys() {
        let mut opake = match build_opake(storage, did).await {
            Ok(o) => o,
            Err(e) => {
                warn!("share-retry: failed to build opake for {did}: {e}");
                continue;
            }
        };

        let transport = ReqwestTransport::new();
        if let Err(e) = opake.retry_pending_shares(&transport).await {
            warn!("share-retry: failed for {did}: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Task: directory sync (apply member proposals to owned workspaces)
// ---------------------------------------------------------------------------

async fn run_directory_sync(storage: &FileStorage) {
    let Some(config) = load_config_or_warn(storage, "directory-sync") else {
        return;
    };

    for did in config.accounts.keys() {
        let mut opake = match build_opake(storage, did).await {
            Ok(o) => o,
            Err(e) => {
                warn!("directory-sync: failed to build opake for {did}: {e}");
                continue;
            }
        };

        match opake.sync_owned_workspaces().await {
            Ok(0) => {}
            Ok(n) => info!("directory-sync: applied {n} proposals for {did}"),
            Err(e) => warn!("directory-sync: failed for {did}: {e}"),
        }
    }
}

async fn build_opake(
    storage: &FileStorage,
    did: &str,
) -> Result<Opake<ReqwestTransport, OsRng, FileStorage>> {
    let mut opake = Opake::for_account(
        storage.clone(),
        Some(did),
        ReqwestTransport::new(),
        OsRng,
        session::chrono_now,
        session::chrono_now_micros,
    )
    .await?;

    if let Ok(url) = std::env::var("OPAKE_APPVIEW_URL") {
        opake.set_appview_url(url);
    }

    Ok(opake)
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// list-tasks — display persisted daemon tasks
// ---------------------------------------------------------------------------

async fn list_tasks(storage: &FileStorage) -> Result<()> {
    use opake_core::daemon::{DaemonTaskKind, TaskStatus};

    let tasks = storage.load_tasks();

    if tasks.is_empty() {
        println!("No background tasks.");
        return Ok(());
    }

    let header = format!("{:<14} {:<16} {:<12} Progress", "ID", "Kind", "Status");
    println!("{header}");
    println!("{}", "-".repeat(60));

    for task in &tasks {
        let (kind, detail) = match &task.kind {
            DaemonTaskKind::SessionRefresh => ("session-refresh", String::new()),
            DaemonTaskKind::PairCleanup { deleted } => {
                ("pair-cleanup", format!("{deleted} deleted"))
            }
            DaemonTaskKind::GrantHealing { healed } => {
                ("grant-healing", format!("{healed} healed"))
            }
            DaemonTaskKind::ShareRetry { retried } => ("share-retry", format!("{retried} retried")),
            DaemonTaskKind::ProposalSync {
                proposals_applied, ..
            } => ("proposal-sync", format!("{proposals_applied} applied")),
            DaemonTaskKind::ReEncryption { .. } => {
                let progress = task.progress.as_ref().map_or(String::new(), |p| {
                    let mb = p.bytes_processed / (1024 * 1024);
                    format!(
                        "{}/{} docs ({mb}MB)",
                        p.completed,
                        p.completed + p.remaining
                    )
                });
                ("re-encryption", progress)
            }
        };
        let status = match &task.status {
            TaskStatus::Pending => "pending".to_string(),
            TaskStatus::Running => "running".to_string(),
            TaskStatus::Completed => "completed".to_string(),
            TaskStatus::Failed(msg) => format!("failed: {msg}"),
        };

        println!(
            "{:<14} {:<16} {:<12} {}",
            &task.id[..task.id.len().min(14)],
            kind,
            status,
            detail
        );
    }

    Ok(())
}

fn load_config_or_warn(storage: &FileStorage, task: &str) -> Option<crate::config::Config> {
    match storage.load_config_anyhow() {
        Ok(c) => Some(c),
        Err(e) => {
            warn!("{task}: failed to load config: {e}");
            None
        }
    }
}
