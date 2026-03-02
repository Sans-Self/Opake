use clap::Args;

use crate::api;
use crate::config::Config;

use super::{build_state, serve_http};

#[derive(Args)]
pub struct ServeCommand {}

impl ServeCommand {
    pub async fn execute(self, config: &Config) -> anyhow::Result<()> {
        let state = build_state(config)?;

        let app = api::router(state);
        serve_http(&config.listen, app, config).await
    }
}
