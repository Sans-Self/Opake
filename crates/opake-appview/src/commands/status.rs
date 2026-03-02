use clap::Args;

use crate::config::Config;
use crate::db;

use super::build_state;

#[derive(Args)]
pub struct StatusCommand {}

impl StatusCommand {
    pub fn execute(self, config: &Config) -> anyhow::Result<()> {
        let state = build_state(config)?;

        let cursor_us = state.db.with_conn(db::cursor::load_cursor).unwrap_or(None);

        let grant_count = state.db.with_conn(db::grants::count_grants).unwrap_or(0);

        let keyring_count = state
            .db
            .with_conn(db::keyrings::count_unique_keyrings)
            .unwrap_or(0);

        match cursor_us {
            Some(us) => {
                let cursor_secs = us / 1_000_000;
                let now_secs = chrono::Utc::now().timestamp();
                let lag_secs = now_secs - cursor_secs;

                let cursor_time = chrono::DateTime::from_timestamp(cursor_secs, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| format!("{us}µs"));

                println!("Cursor:   {cursor_time}");
                println!("Lag:      {lag_secs}s");
            }
            None => {
                println!("Cursor:   (none — indexer has not run)");
            }
        }

        println!("Grants:   {grant_count}");
        println!("Keyrings: {keyring_count}");
        Ok(())
    }
}
