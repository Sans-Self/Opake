mod service;

use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;
use clap::{Args, Subcommand};
use log::{info, warn};
use opake_core::client::ReqwestTransport;
use opake_core::crypto::{OsRng, RngCore};
use opake_core::indexer::daemon::{self, TASKS};
use opake_core::indexer::sse::consumer::{JitterRng, SleepFn, SseConsumer, TokenFetcher};
use opake_core::indexer::sse::events::SseEvent;
use opake_core::indexer::sse::reqwest_connection::ReqwestSseTransport;
use opake_core::opake::Opake;
use tokio::sync::{Mutex, Notify};
use tokio::task::LocalSet;

use crate::config::FileStorage;
use crate::session::build_opake;

/// Shared handle to a CLI-side Opake. Held behind `tokio::sync::Mutex`
/// so the SSE consumer's token fetcher and the event handler serialize
/// correctly — only one can hold the mutable reference at a time. `Rc`
/// (not `Arc`) because each consumer runs inside a `LocalSet` and
/// never crosses thread boundaries.
type SharedOpake = Rc<Mutex<Opake<ReqwestTransport, OsRng, FileStorage>>>;

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
    println!("opake daemon starting ({task_summary}, sync=SSE)");

    // Everything runs inside a LocalSet so the SSE consumer tasks (which
    // are `!Send` — the `TokenFetcher` trait object doesn't carry a Send
    // bound to stay compatible with WASM) can use `tokio::task::spawn_local`.
    let local = LocalSet::new();
    local
        .run_until(async move {
            let cancel = Rc::new(Notify::new());

            // Spawn one long-lived SSE consumer per configured account.
            // Each consumer does an initial catch-up sync on connect,
            // then streams events from the indexer and applies proposals
            // as they arrive. Record events (DirectoryUpsert,
            // DocumentUpsert, etc.) are dropped — the CLI has no
            // TreeKeeper or UI that needs live tree state.
            if let Some(config) = load_config_or_warn(storage, "sync") {
                for did in config.accounts.keys() {
                    let storage = storage.clone();
                    let did = did.clone();
                    let cancel = Rc::clone(&cancel);
                    tokio::task::spawn_local(async move {
                        run_sync_consumer_for_did(&storage, &did, cancel).await;
                    });
                }
            }

            // Maintenance intervals run alongside the consumer tasks.
            // These rebuild Opake per tick (fresh session read from
            // storage each time) and are short-lived per invocation.
            let mut pair_tick = tokio::time::interval(task_interval("pair-cleanup"));
            let mut grant_tick = tokio::time::interval(task_interval("grant-healing"));
            let mut share_tick = tokio::time::interval(task_interval("share-retry"));
            let mut rewrap_tick = tokio::time::interval(task_interval("rotation-rewrap"));

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
                    _ = rewrap_tick.tick() => {
                        info!("running: rotation-rewrap");
                        run_rotation_rewrap(storage).await;
                    }
                    _ = tokio::signal::ctrl_c() => {
                        info!("received SIGINT, shutting down");
                        println!("shutting down");
                        cancel.notify_waiters();
                        // Give the spawned consumers a moment to observe
                        // cancellation and exit cleanly. Their next
                        // `next_event().await` yield is the cancel point;
                        // 250ms is far more than enough for the select
                        // arm to pick it up.
                        tokio::time::sleep(Duration::from_millis(250)).await;
                        break;
                    }
                }
            }
        })
        .await;

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
// Task: rotation re-wrap sweep
// ---------------------------------------------------------------------------

async fn run_rotation_rewrap(storage: &FileStorage) {
    let Some(config) = load_config_or_warn(storage, "rotation-rewrap") else {
        return;
    };

    for did in config.accounts.keys() {
        let mut opake = match build_opake(storage, did).await {
            Ok(o) => o,
            Err(e) => {
                warn!("rotation-rewrap: failed to build opake for {did}: {e}");
                continue;
            }
        };

        match opake.sweep_owned_documents_rewrap().await {
            Ok(outcome) if outcome.rewrapped > 0 => {
                info!(
                    "rotation-rewrap: {} document(s) migrated for {did} ({} conflicts)",
                    outcome.rewrapped, outcome.conflicts
                );
            }
            Ok(_) => {}
            Err(e) => warn!("rotation-rewrap: failed for {did}: {e}"),
        }
    }
}

// ---------------------------------------------------------------------------
// SSE consumer — long-lived sync task, one per configured DID
// ---------------------------------------------------------------------------
//
// Replaces the old `directory-sync` timer-based task. Flow per DID:
//
//   1. Build an Opake (reads session from storage)
//   2. Initial catch-up: call `sync_owned_workspaces_detailed` to load
//      chain heads for every workspace we're a member of (warms the
//      indexer's tree cache before the live stream opens)
//   3. Start an `SseConsumer` loop over `ReqwestSseTransport`
//   4. On each event:
//      - `SseEvent::Reconnect` → another `sync_owned_workspaces_detailed`
//        to catch anything we missed during the disconnect. Phoenix
//        PubSub doesn't buffer events for offline subscribers, so the
//        catch-up sync is what makes reconnection "not lossy."
//      - record events + chain-fork events → drop silently. The CLI
//        has no TreeKeeper or UI that needs live tree state.
//   5. On cancellation (SIGINT from the main loop), exit cleanly.
//
// Session refresh: OAuth tokens are managed internally by the XrpcClient
// on 401 responses, so a long-lived Opake recovers from stale sessions
// transparently. The `session-refresh` interval task independently
// updates storage — our Opake may briefly hold a stale in-memory session
// but the XRPC client heals on the next call. SSE tokens themselves are
// Ed25519-signed from the identity, which is stable for the account's
// lifetime.

async fn run_sync_consumer_for_did(storage: &FileStorage, did: &str, cancel: Rc<Notify>) {
    let opake = match build_opake(storage, did).await {
        Ok(o) => o,
        Err(e) => {
            warn!("sync: failed to build opake for {did}: {e}");
            return;
        }
    };

    let indexer_url = opake.resolve_indexer_url();

    let opake = Rc::new(Mutex::new(opake));

    // Initial catch-up before opening the event stream. Loads chain
    // heads for every workspace we're a member of so the indexer's
    // tree cache is warm before the live stream opens.
    {
        let mut guard = opake.lock().await;
        match guard.sync_owned_workspaces_detailed().await {
            Ok(results) => {
                if !results.is_empty() {
                    info!(
                        "sync: initial catch-up loaded {} workspaces for {did}",
                        results.len()
                    );
                }
            }
            Err(e) => {
                warn!("sync: initial catch-up failed for {did}: {e}");
                // Non-fatal — we still start the consumer. A later
                // reconnect or event will trigger another sync.
            }
        }
    }

    // Build the consumer's dependencies. The token fetcher and sleep
    // function are Box<dyn FnMut>s, owned by the consumer.
    let token_fetcher = make_native_token_fetcher(Rc::clone(&opake));
    let sleep_fn: SleepFn = Box::new(|d| Box::pin(tokio::time::sleep(d)));
    let jitter_fn: JitterRng = Box::new(|| {
        let u = OsRng.next_u64();
        (u as f64) / (u64::MAX as f64 + 1.0)
    });

    let transport = ReqwestSseTransport::with_default_client();
    let mut consumer = SseConsumer::new(transport, indexer_url, token_fetcher, sleep_fn, jitter_fn);

    info!("sync: consumer started for {did}");

    loop {
        tokio::select! {
            _ = cancel.notified() => {
                info!("sync: consumer stopping for {did}");
                return;
            }
            event_result = consumer.next_event() => {
                match event_result {
                    Ok(event) => handle_sse_event(&opake, event, did).await,
                    Err(e) => {
                        // The consumer only returns Err for fatal errors
                        // (all recoverable ones are handled via internal
                        // backoff). Log and exit — another daemon run
                        // will restart us.
                        warn!("sync: consumer terminated for {did}: {e}");
                        return;
                    }
                }
            }
        }
    }
}

/// Dispatch an SSE event to the shared Opake for this DID.
///
/// Reconnect triggers a full catch-up across every owned workspace (same
/// as initial). All other event variants are dropped — the CLI has no
/// TreeKeeper to patch. The pre-federation proposal-dispatch and
/// proposal-cleanup paths are gone with the federation rewrite; chain-fork
/// retry will land alongside the cascade-aware SDK.
async fn handle_sse_event(opake: &SharedOpake, event: SseEvent, did: &str) {
    if matches!(event, SseEvent::Reconnect) {
        let mut guard = opake.lock().await;
        match guard.sync_owned_workspaces_detailed().await {
            Ok(_) => {}
            Err(e) => {
                warn!("sync: reconnect catch-up failed for {did}: {e}");
            }
        }
    }
    // Other record events (GrantUpsert, deletes, ChainForked, etc.) are
    // dropped intentionally. The CLI has no TreeKeeper or UI that needs
    // live tree state.
}

/// Build a token fetcher that uses the shared Opake to request a fresh
/// SSE token on every connect attempt. Mirrors the WASM-side
/// `make_token_fetcher` in `crates/opake-wasm/src/sse_wasm.rs`.
fn make_native_token_fetcher(opake: SharedOpake) -> TokenFetcher {
    Box::new(move || {
        let opake = Rc::clone(&opake);
        Box::pin(async move {
            let mut guard = opake.lock().await;
            guard.request_sse_token().await
        })
    })
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// list-tasks — display persisted daemon tasks
// ---------------------------------------------------------------------------

async fn list_tasks(storage: &FileStorage) -> Result<()> {
    use opake_core::indexer::daemon::{DaemonTaskKind, TaskStatus};

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
            DaemonTaskKind::RotationRewrap { rewrapped } => {
                ("rotation-rewrap", format!("{rewrapped} re-wrapped"))
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
