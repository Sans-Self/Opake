use clap::Args;

use crate::api;
use crate::config::Config;
use crate::indexer;

use super::{build_state, serve_http};

#[derive(Args)]
pub struct RunCommand {}

impl RunCommand {
    pub async fn execute(self, config: &Config) -> anyhow::Result<()> {
        let state = build_state(config)?;

        tokio::spawn({
            let state = state.clone();
            let url = config.jetstream_url.clone();
            async move { indexer::run(state, url).await }
        });

        let app = api::router(state);
        serve_http(&config.listen, app, config).await
    }
}
