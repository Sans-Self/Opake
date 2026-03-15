mod commands;
mod config;
mod document_resolve;
mod identity;
mod keyring_store;
mod oauth;
mod prompt;
mod session;
pub mod utils;

use clap::builder::styling::{AnsiColor, Styles};
use clap::{CommandFactory, Parser, Subcommand};
use commands::Execute;
use config::FileStorage;
use log::info;

const fn opake_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::BrightCyan.on_default().bold())
        .usage(AnsiColor::BrightCyan.on_default())
        .literal(AnsiColor::BrightWhite.on_default().bold())
        .placeholder(AnsiColor::BrightMagenta.on_default())
        .valid(AnsiColor::BrightGreen.on_default())
        .invalid(AnsiColor::BrightRed.on_default())
        .error(AnsiColor::BrightRed.on_default().bold())
}

/// Encrypted personal cloud on AT Protocol.
///
/// Opake encrypts files client-side and stores them on your AT Protocol PDS.
/// All crypto happens locally — the server only ever sees ciphertext.
#[derive(Parser)]
#[command(
    name = "opake",
    version,
    author = "Not Herself <me@sans-self.org>",
    styles = opake_styles(),
    after_help = "\
Quick start:
  opake account login alice.bsky.social
  opake upload secret.pdf
  opake ls
  opake share new secret.pdf bob.bsky.social

https://opake.app",
)]
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
    // --- Grouped commands ---
    Account(commands::account::AccountCommand),
    Share(commands::share_group::ShareGroupCommand),
    Keyring(commands::keyring::KeyringCommand),
    Metadata(commands::metadata::MetadataCommand),
    Pair(commands::pair::PairCommand),

    // --- Files ---
    Upload(commands::upload::UploadCommand),
    Download(commands::download::DownloadCommand),
    Cat(commands::cat::CatCommand),
    Ls(commands::ls::LsCommand),
    Tree(commands::tree::TreeCommand),
    Rm(commands::rm::RmCommand),
    Move(commands::move_cmd::MoveCommand),
    Mkdir(commands::mkdir::MkdirCommand),

    // --- Danger Zone ---
    Purge(commands::purge::PurgeCommand),

    // --- Utilities ---
    Config(commands::config::ConfigCommand),
    Recover(commands::recover::RecoverCommand),
    Resolve(commands::resolve::ResolveCommand),
    Completions(commands::completions::CompletionsCommand),
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
        Command::Account(cmd) => {
            let session = cmd.execute(&storage).await?;
            if let Some(ref s) = session {
                session::persist_session(&storage, s.did(), s)?;
            }
        }

        Command::Upload(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Download(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Cat(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Ls(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Metadata(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Mkdir(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Move(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Rm(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Tree(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,

        Command::Share(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Keyring(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,

        Command::Config(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Pair(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Purge(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Recover(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Resolve(cmd) => run_with_context(&storage, as_flag.as_deref(), cmd).await?,
        Command::Completions(cmd) => cmd.run(&mut Cli::command()),
    }

    Ok(())
}
