pub mod download;
pub mod login;
pub mod ls;
pub mod rm;
pub mod upload;

use anyhow::Result;
use opake_core::client::Session;

pub trait Execute {
    fn execute(self) -> impl std::future::Future<Output = Result<Option<Session>>>;
}
