pub mod accounts;
pub mod download;
pub mod inbox;
pub mod keyring;
pub mod login;
pub mod logout;
pub mod ls;
pub mod resolve;
pub mod revoke;
pub mod rm;
pub mod set_default;
pub mod share;
pub mod shared;
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
