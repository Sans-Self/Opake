mod api;
mod commands;
mod config;
mod db;
mod error;
mod firehose;
mod indexer;
mod state;

use std::path::PathBuf;

use clap::Parser;

use commands::Command;

#[derive(Parser)]
#[command(name = "opake-appview", about = "AppView indexer and API for Opake")]
struct Cli {
    /// Increase output verbosity (-v info, -vv debug, -vvv trace)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Override config directory (where appview.toml lives)
    #[arg(long, global = true)]
    config_dir: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let log_level = match cli.verbose {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Info,
        2 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };

    env_logger::Builder::new()
        .filter_level(log_level)
        .parse_default_env()
        .init();

    let config = config::Config::load(cli.config_dir.map(PathBuf::from))?;

    match cli
        .command
        .unwrap_or(Command::Run(commands::run::RunCommand {}))
    {
        Command::Run(cmd) => cmd.execute(&config).await,
        Command::Index(cmd) => cmd.execute(&config).await,
        Command::Serve(cmd) => cmd.execute(&config).await,
        Command::Status(cmd) => cmd.execute(&config),
    }
}
