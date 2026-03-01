mod commands;
mod config;
mod identity;
mod session;
mod transport;
pub mod utils;

use clap::{Parser, Subcommand};
use commands::Execute;
use log::info;

#[derive(Parser)]
#[command(name = "opake", about = "Encrypted personal cloud on AT Protocol")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Login(commands::login::LoginCommand),
    Upload(commands::upload::UploadCommand),
    Download(commands::download::DownloadCommand),
    Ls(commands::ls::LsCommand),
    Rm(commands::rm::RmCommand),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    info!("Starting Opake CLI. Hello!");
    let cli = Cli::parse();

    let refreshed = match cli.command {
        Command::Login(cmd) => cmd.execute().await?,
        Command::Upload(cmd) => cmd.execute().await?,
        Command::Download(cmd) => cmd.execute().await?,
        Command::Ls(cmd) => cmd.execute().await?,
        Command::Rm(cmd) => cmd.execute().await?,
    };

    if let Some(ref s) = refreshed {
        session::persist_session(&s.did, s)?;
    }

    Ok(())
}
