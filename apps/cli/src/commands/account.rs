use clap::{Args, Subcommand};
use opake_core::client::Session;

use crate::config::FileStorage;

use super::{accounts, login, logout, session_cmd, set_default};

/// Manage accounts and authentication
#[derive(Args)]
pub struct AccountCommand {
    #[command(subcommand)]
    action: AccountAction,
}

#[derive(Subcommand)]
enum AccountAction {
    /// Authenticate with your PDS
    Login(login::LoginCommand),
    /// Remove a local account (PDS data is not affected)
    Logout(logout::LogoutCommand),
    /// List all logged-in accounts (* marks the default)
    List(accounts::AccountsCommand),
    /// Set the default account (used when --as is omitted)
    SetDefault(set_default::SetDefaultCommand),
    /// Manage session tokens
    Session(session_cmd::SessionCommand),
}

impl AccountCommand {
    pub async fn execute(self, storage: &FileStorage) -> anyhow::Result<Option<Session>> {
        match self.action {
            AccountAction::Login(cmd) => cmd.execute(storage).await,
            AccountAction::Logout(cmd) => {
                cmd.run(storage)?;
                Ok(None)
            }
            AccountAction::List(cmd) => {
                cmd.run(storage)?;
                Ok(None)
            }
            AccountAction::SetDefault(cmd) => {
                cmd.run(storage)?;
                Ok(None)
            }
            AccountAction::Session(cmd) => cmd.execute(storage).await,
        }
    }
}
