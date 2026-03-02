use rusqlite::{params, Connection};

use crate::error::Result;

/// Save the Jetstream cursor (unix microseconds timestamp).
/// Uses upsert into the singleton row (id = 1).
pub fn save_cursor(conn: &Connection, time_us: i64) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO cursor (id, time_us, updated_at)
         VALUES (1, ?1, ?2)
         ON CONFLICT(id) DO UPDATE SET
           time_us = excluded.time_us,
           updated_at = excluded.updated_at",
        params![time_us, now],
    )?;
    Ok(())
}

/// Load the last saved Jetstream cursor, if any.
pub fn load_cursor(conn: &Connection) -> Result<Option<i64>> {
    let mut stmt = conn.prepare("SELECT time_us FROM cursor WHERE id = 1")?;
    let result = stmt.query_row([], |row| row.get::<_, i64>(0));
    match result {
        Ok(time_us) => Ok(Some(time_us)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
