pub mod cursor;
pub mod grants;
pub mod keyrings;
mod schema;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::Result;

/// Thread-safe database handle. Axum handlers and the indexer share this.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open (or create) the SQLite database at the given path and run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(schema::SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Create an in-memory database for testing.
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(schema::SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Run a closure with a reference to the connection.
    /// Panics if the mutex is poisoned (unrecoverable).
    pub fn with_conn<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&Connection) -> T,
    {
        let conn = self.conn.lock().expect("database mutex poisoned");
        f(&conn)
    }
}

#[cfg(test)]
#[path = "db_tests.rs"]
mod tests;
