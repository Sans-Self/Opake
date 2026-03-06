mod commands;
mod config;
mod identity;
mod keyring_store;
mod oauth;
mod session;
mod transport;
pub mod utils;

use clap::{Parser, Subcommand};
use commands::Execute;
use config::FileStorage;
use log::info;

#[derive(Parser)]
#[command(name = "opake", about = "Encrypted personal cloud on AT Protocol")]
struct Cli {
    /// Act as a specific account (handle or DID)
    #[arg(long, global = true)]
    r#as: Option<String>,

    /// Override config directory
    #[arg(long, global = true)]
    config_dir: Option<String>,

    /// Increase output verbosity (-v info, -vv debug, -vvv trace)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

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
    Cat(commands::cat::CatCommand),
    Inbox(commands::inbox::InboxCommand),
    Ls(commands::ls::LsCommand),
    Mkdir(commands::mkdir::MkdirCommand),
    Mv(commands::mv::MvCommand),
    Rm(commands::rm::RmCommand),
    Resolve(commands::resolve::ResolveCommand),
    Share(commands::share::ShareCommand),
    Shared(commands::shared::SharedCommand),
    Revoke(commands::revoke::RevokeCommand),
    Keyring(commands::keyring::KeyringCommand),
    Pair(commands::pair::PairCommand),
    Tree(commands::tree::TreeCommand),
}

async fn run_with_context(
    storage: &FileStorage,
    as_flag: Option<&str>,
    cmd: impl Execute,
) -> anyhow::Result<()> {
    let ctx = session::resolve_context(storage, as_flag)?;
    let refreshed = cmd.execute(&ctx).await?;
    if let Some(ref s) = refreshed {
        session::persist_session(&ctx.storage, s.did(), s)?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Cli {
        r#as: as_flag,
        config_dir,
        verbose,
        command,
    } = Cli::parse();

    let base_dir = opake_core::paths::resolve_data_dir(config_dir.map(Into::into));

    let log_level = match verbose {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Info,
        2 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };
    env_logger::Builder::new()
        .filter_level(log_level)
        .parse_default_env()
        .init();

    info!("Starting Opake CLI. Hello!");

    let storage = FileStorage::new(base_dir);

    match command {
        Command::Login(cmd) => {
            let session = cmd.execute(&storage).await?;
            if let Some(ref s) = session {
                session::persist_session(&storage, s.did(), s)?;
            }
        }
        Command::Logout(cmd) => cmd.run(&storage)?,
        Command::Accounts(cmd) => cmd.run(&storage)?,
        Command::SetDefault(cmd) => cmd.run(&storage)?,

        Command::Upload(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Download(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Cat(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Inbox(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Ls(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Mkdir(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Mv(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Rm(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Resolve(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Share(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Shared(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Revoke(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Keyring(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Pair(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Tree(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
    }

    Ok(())
}
