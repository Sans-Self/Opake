pub mod download;
pub mod login;
pub mod ls;
pub mod rm;
pub mod upload;

use anyhow::Result;

pub trait Execute {
    fn execute(self) -> impl std::future::Future<Output = Result<()>>;
}
