use rusqlite::{params, Connection};

use crate::error::Result;

/// A keyring membership row as stored in the index.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct IndexedKeyringMember {
    pub keyring_uri: String,
    pub member_did: String,
    pub owner_did: String,
    pub indexed_at: String,
}

/// Replace all members for a keyring (delete-and-reinsert).
/// This handles both create and update events correctly — on update,
/// the member list may have changed, so we wipe and rewrite.
pub fn upsert_keyring_members(
    conn: &Connection,
    keyring_uri: &str,
    owner_did: &str,
    member_dids: &[String],
    indexed_at: &str,
) -> Result<()> {
    conn.execute(
        "DELETE FROM keyring_members WHERE keyring_uri = ?1",
        params![keyring_uri],
    )?;

    let mut stmt = conn.prepare(
        "INSERT INTO keyring_members (keyring_uri, member_did, owner_did, indexed_at)
         VALUES (?1, ?2, ?3, ?4)",
    )?;

    for did in member_dids {
        stmt.execute(params![keyring_uri, did, owner_did, indexed_at])?;
    }

    Ok(())
}

/// Delete all member rows for a keyring (when the record is deleted).
pub fn delete_keyring(conn: &Connection, keyring_uri: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM keyring_members WHERE keyring_uri = ?1",
        params![keyring_uri],
    )?;
    Ok(())
}

/// Paginated query: keyrings where a DID is a member, newest first.
/// Returns one row per unique keyring (not per member).
pub fn list_keyrings_for_member(
    conn: &Connection,
    member_did: &str,
    limit: u32,
    cursor: Option<&str>,
) -> Result<Vec<IndexedKeyringMember>> {
    let mut keyrings = Vec::new();

    if let Some(cursor) = cursor {
        let (cursor_time, cursor_uri) = parse_cursor(cursor);
        let mut stmt = conn.prepare(
            "SELECT keyring_uri, member_did, owner_did, indexed_at
             FROM keyring_members
             WHERE member_did = ?1
               AND (indexed_at < ?2 OR (indexed_at = ?2 AND keyring_uri < ?3))
             ORDER BY indexed_at DESC, keyring_uri DESC
             LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![member_did, cursor_time, cursor_uri, limit],
            row_to_member,
        )?;
        for row in rows {
            keyrings.push(row?);
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT keyring_uri, member_did, owner_did, indexed_at
             FROM keyring_members
             WHERE member_did = ?1
             ORDER BY indexed_at DESC, keyring_uri DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![member_did, limit], row_to_member)?;
        for row in rows {
            keyrings.push(row?);
        }
    }

    Ok(keyrings)
}

fn row_to_member(row: &rusqlite::Row) -> rusqlite::Result<IndexedKeyringMember> {
    Ok(IndexedKeyringMember {
        keyring_uri: row.get(0)?,
        member_did: row.get(1)?,
        owner_did: row.get(2)?,
        indexed_at: row.get(3)?,
    })
}

/// Build a cursor string from an indexed keyring member.
pub fn encode_cursor(member: &IndexedKeyringMember) -> String {
    format!("{}::{}", member.indexed_at, member.keyring_uri)
}

/// Count unique keyrings in the index.
pub fn count_unique_keyrings(conn: &Connection) -> Result<i64> {
    let count = conn.query_row(
        "SELECT COUNT(DISTINCT keyring_uri) FROM keyring_members",
        [],
        |row| row.get(0),
    )?;
    Ok(count)
}

fn parse_cursor(cursor: &str) -> (&str, &str) {
    match cursor.split_once("::") {
        Some((time, uri)) => (time, uri),
        None => (cursor, ""),
    }
}
