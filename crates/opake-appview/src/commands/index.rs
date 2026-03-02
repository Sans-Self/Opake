use clap::Args;

use crate::config::Config;
use crate::indexer;

use super::build_state;

#[derive(Args)]
pub struct IndexCommand {}

impl IndexCommand {
    pub async fn execute(self, config: &Config) -> anyhow::Result<()> {
        let state = build_state(config)?;

        log::info!(
            "opake-appview indexer (db: {})",
            config.resolved_db_path().display()
        );

        indexer::run(state, config.jetstream_url.clone()).await;
        Ok(())
    }
}
