use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::db::cursor;
use crate::db::grants::{self, IndexedGrant};
use crate::db::keyrings;
use crate::firehose::events::{self, IndexableEvent};
use crate::firehose::subscribe;
use crate::state::AppState;

const CURSOR_SAVE_INTERVAL: u64 = 100;
const MAX_BACKOFF_SECS: u64 = 60;

/// Run the indexer loop. Connects to Jetstream, processes events, writes to DB.
/// Reconnects with exponential backoff on failure. Runs until the task is cancelled.
pub async fn run(state: Arc<AppState>, jetstream_url: String) {
    let mut backoff_secs: u64 = 1;

    loop {
        let cursor_us = state.db.with_conn(cursor::load_cursor).unwrap_or(None);

        let url = subscribe::subscription_url(&jetstream_url, cursor_us);

        match subscribe::connect(&url).await {
            Ok(mut stream) => {
                backoff_secs = 1;
                state.indexer_connected.store(true, Ordering::Relaxed);
                log::info!("connected to jetstream, indexing events");

                let mut events_since_cursor_save: u64 = 0;

                loop {
                    match subscribe::next_message(&mut stream).await {
                        Ok(Some(text)) => {
                            if let Some((event, time_us)) = events::parse_event(&text) {
                                if let Err(e) = process_event(&state, &event, time_us) {
                                    log::error!("failed to process event: {e}");
                                    continue;
                                }
                                events_since_cursor_save += 1;
                                if events_since_cursor_save >= CURSOR_SAVE_INTERVAL {
                                    if let Err(e) =
                                        state.db.with_conn(|c| cursor::save_cursor(c, time_us))
                                    {
                                        log::error!("failed to save cursor: {e}");
                                    }
                                    events_since_cursor_save = 0;
                                }
                            }
                        }
                        Ok(None) => {
                            log::warn!("jetstream stream closed, reconnecting");
                            break;
                        }
                        Err(e) => {
                            log::error!("jetstream read error: {e}");
                            break;
                        }
                    }
                }

                state.indexer_connected.store(false, Ordering::Relaxed);
            }
            Err(e) => {
                log::error!("jetstream connection failed: {e}");
            }
        }

        log::info!("reconnecting in {backoff_secs}s");
        tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(MAX_BACKOFF_SECS);
    }
}

fn process_event(
    state: &AppState,
    event: &IndexableEvent,
    _time_us: i64,
) -> crate::error::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();

    state.db.with_conn(|conn| match event {
        IndexableEvent::UpsertGrant {
            uri,
            owner_did,
            recipient_did,
            document_uri,
            created_at,
        } => {
            let grant = IndexedGrant {
                uri: uri.clone(),
                owner_did: owner_did.clone(),
                recipient_did: recipient_did.clone(),
                document_uri: document_uri.clone(),
                created_at: created_at.clone(),
                indexed_at: now.clone(),
            };
            grants::upsert_grant(conn, &grant)?;
            log::debug!("indexed grant: {uri}");
            Ok(())
        }
        IndexableEvent::DeleteGrant { uri } => {
            grants::delete_grant(conn, uri)?;
            log::debug!("deleted grant: {uri}");
            Ok(())
        }
        IndexableEvent::UpsertKeyring {
            uri,
            owner_did,
            member_dids,
        } => {
            keyrings::upsert_keyring_members(conn, uri, owner_did, member_dids, &now)?;
            log::debug!("indexed keyring: {uri} ({} members)", member_dids.len());
            Ok(())
        }
        IndexableEvent::DeleteKeyring { uri } => {
            keyrings::delete_keyring(conn, uri)?;
            log::debug!("deleted keyring: {uri}");
            Ok(())
        }
    })
}

#[cfg(test)]
#[path = "indexer_tests.rs"]
mod tests;
