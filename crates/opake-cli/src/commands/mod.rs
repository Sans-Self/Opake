pub mod account;
pub mod accounts;
pub mod cat;
pub mod completions;
pub mod config;
pub mod download;
pub mod inbox;
pub mod keyring;
pub mod login;
pub mod logout;
pub mod ls;
pub mod metadata;
pub mod mkdir;
pub mod move_cmd;
pub mod pair;
pub mod purge;
pub mod recover;
pub mod resolve;
pub mod revoke;
pub mod rm;
pub mod set_default;
pub mod share;
pub mod share_group;
pub mod shared;
pub mod tree;
pub mod upload;

use anyhow::Result;
use opake_core::client::Session;

use crate::session::CommandContext;

pub trait Execute {
    fn execute(
        self,
        ctx: &CommandContext,
    ) -> impl std::future::Future<Output = Result<Option<Session>>>;
}

/// Re-export for CLI commands that need to build directory encryption envelopes.
pub use opake_core::directories::encrypt_directory_envelope as encrypt_directory;
