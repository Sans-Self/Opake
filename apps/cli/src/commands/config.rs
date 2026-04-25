use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use opake_core::client::Session;
use opake_core::records::AccountConfigRecord;

use crate::commands::Execute;
use crate::session::CommandContext;

/// View or modify account config synced to your PDS
///
/// Account config is stored as a record on your PDS and syncs across
/// devices. Local settings (default account) are not affected.
#[derive(Args)]
#[command(after_help = "\
Examples:
  opake config
  opake config set telemetry-enabled true
  opake config set indexer-url https://indexer.opake.app")]
pub struct ConfigCommand {
    #[command(subcommand)]
    action: Option<ConfigAction>,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Update a config value
    Set(SetArgs),
}

#[derive(Args)]
struct SetArgs {
    /// Config key to update
    key: String,
    /// New value
    value: String,
}

const VALID_KEYS: &[&str] = &["indexer-url", "telemetry-enabled"];

fn parse_bool(value: &str) -> Result<bool> {
    match value {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => bail!("expected a boolean (true/false), got: {value}"),
    }
}

impl Execute for ConfigCommand {
    async fn execute(self, ctx: &CommandContext) -> Result<Option<Session>> {
        let mut opake = ctx.opake().await?;

        match self.action {
            None => {
                let config = opake.get_account_config().await?;
                print_config(config.as_ref());
            }
            Some(ConfigAction::Set(args)) => {
                let now = opake.now();
                let mut config = opake
                    .get_account_config()
                    .await?
                    .unwrap_or_else(|| AccountConfigRecord::new(&now));

                match args.key.as_str() {
                    "indexer-url" => {
                        let url = args.value.trim().to_string();
                        config.indexer_url = if url.is_empty() { None } else { Some(url) };
                    }
                    "telemetry-enabled" => {
                        config.telemetry_enabled = parse_bool(&args.value)?;
                    }
                    _ => bail!(
                        "unknown config key: {}\n\nvalid keys:\n  {}",
                        args.key,
                        VALID_KEYS.join("\n  ")
                    ),
                }

                config.modified_at = now;
                opake.set_account_config(&config).await?;

                print_config(Some(&config));
            }
        }

        Ok(None)
    }
}

fn print_config(config: Option<&AccountConfigRecord>) {
    match config {
        Some(config) => {
            let telemetry = if config.telemetry_enabled {
                "enabled"
            } else {
                "disabled"
            };
            println!("telemetry    {telemetry}");
            println!(
                "indexer      {}",
                config.indexer_url.as_deref().unwrap_or("(not set)")
            );
            println!("modified     {}", config.modified_at);
        }
        None => {
            println!("telemetry    disabled  (default)");
            println!("indexer      (not set)");
            println!("modified     never");
        }
    }
}
