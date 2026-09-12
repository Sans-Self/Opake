pub mod account;
pub mod accounts;
pub mod cat;
pub mod completions;
pub mod config;
pub mod daemon;
pub mod download;
pub mod inbox;
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
pub mod session_cmd;
pub mod set_default;
pub mod share;
pub mod share_group;
pub mod shared;
pub mod tree;
pub mod upload;
pub mod verification;
pub mod workspace;

use anyhow::Result;
use opake_core::client::Session;

use crate::session::CommandContext;

pub trait Execute {
    fn execute(
        self,
        ctx: &CommandContext,
    ) -> impl std::future::Future<Output = Result<Option<Session>>>;
}
