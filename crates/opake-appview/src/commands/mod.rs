pub mod index;
pub mod run;
pub mod serve;
pub mod status;

use std::sync::Arc;

use anyhow::Context;
use clap::Subcommand;

use crate::config::Config;
use crate::db::Database;
use crate::state::AppState;

#[derive(Subcommand)]
pub enum Command {
    /// Run both indexer and API server (default)
    Run(run::RunCommand),
    /// Run indexer only (write-only, no HTTP server)
    Index(index::IndexCommand),
    /// Run API server only (read-only, no Jetstream connection)
    Serve(serve::ServeCommand),
    /// Print cursor position, lag, and stats, then exit
    Status(status::StatusCommand),
}

pub fn build_state(config: &Config) -> anyhow::Result<Arc<AppState>> {
    let db = Database::open(&config.resolved_db_path()).context("failed to open database")?;
    Ok(Arc::new(AppState::new(db)))
}

pub async fn serve_http(listen: &str, app: axum::Router, config: &Config) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .with_context(|| format!("failed to bind to {listen}"))?;

    log::info!(
        "opake-appview listening on {} (db: {})",
        listen,
        config.resolved_db_path().display()
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    log::info!("shutting down");
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for ctrl-c");
}
