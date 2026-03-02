use std::sync::atomic::AtomicBool;

use tokio::sync::Mutex;

use crate::api::key_cache::KeyCache;
use crate::db::Database;

/// Shared application state for Axum handlers and the indexer.
pub struct AppState {
    pub db: Database,
    pub indexer_connected: AtomicBool,
    pub key_cache: Mutex<KeyCache>,
}

impl AppState {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            indexer_connected: AtomicBool::new(false),
            key_cache: Mutex::new(KeyCache::new()),
        }
    }
}
