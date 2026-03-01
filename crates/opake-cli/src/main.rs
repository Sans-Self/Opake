mod commands;
mod config;
mod identity;
mod keyring_store;
mod session;
mod transport;
pub mod utils;

use clap::{Parser, Subcommand};
use commands::Execute;
use log::info;

#[derive(Parser)]
#[command(name = "opake", about = "Encrypted personal cloud on AT Protocol")]
struct Cli {
    /// Act as a specific account (handle or DID)
    #[arg(long, global = true)]
    r#as: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Login(commands::login::LoginCommand),
    Logout(commands::logout::LogoutCommand),
    Accounts(commands::accounts::AccountsCommand),
    SetDefault(commands::set_default::SetDefaultCommand),
    Upload(commands::upload::UploadCommand),
    Download(commands::download::DownloadCommand),
    Ls(commands::ls::LsCommand),
    Rm(commands::rm::RmCommand),
    Resolve(commands::resolve::ResolveCommand),
    Share(commands::share::ShareCommand),
    Shared(commands::shared::SharedCommand),
    Revoke(commands::revoke::RevokeCommand),
    Keyring(commands::keyring::KeyringCommand),
}

async fn run_with_context(as_flag: Option<&str>, cmd: impl Execute) -> anyhow::Result<()> {
    let ctx = session::resolve_context(as_flag)?;
    let refreshed = cmd.execute(&ctx).await?;
    if let Some(ref s) = refreshed {
        session::persist_session(&s.did, s)?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    info!("Starting Opake CLI. Hello!");
    let Cli {
        r#as: as_flag,
        command,
    } = Cli::parse();

    match command {
        Command::Login(cmd) => {
            let session = cmd.execute().await?;
            if let Some(ref s) = session {
                session::persist_session(&s.did, s)?;
            }
        }
        Command::Logout(cmd) => cmd.run()?,
        Command::Accounts(cmd) => cmd.run()?,
        Command::SetDefault(cmd) => cmd.run()?,

        Command::Upload(cmd) => run_with_context(as_flag.as_deref(), cmd).await?,
        Command::Download(cmd) => run_with_context(as_flag.as_deref(), cmd).await?,
        Command::Ls(cmd) => run_with_context(as_flag.as_deref(), cmd).await?,
        Command::Rm(cmd) => run_with_context(as_flag.as_deref(), cmd).await?,
        Command::Resolve(cmd) => run_with_context(as_flag.as_deref(), cmd).await?,
        Command::Share(cmd) => run_with_context(as_flag.as_deref(), cmd).await?,
        Command::Shared(cmd) => run_with_context(as_flag.as_deref(), cmd).await?,
        Command::Revoke(cmd) => run_with_context(as_flag.as_deref(), cmd).await?,
        Command::Keyring(cmd) => run_with_context(as_flag.as_deref(), cmd).await?,
    }

    Ok(())
}
