use rusqlite::{params, Connection};

use crate::error::Result;

/// A grant row as stored in the index.
#[derive(Debug, Clone)]
pub struct IndexedGrant {
    pub uri: String,
    pub owner_did: String,
    pub recipient_did: String,
    pub document_uri: String,
    pub created_at: String,
    pub indexed_at: String,
}

pub fn upsert_grant(conn: &Connection, grant: &IndexedGrant) -> Result<()> {
    conn.execute(
        "INSERT INTO grants (uri, owner_did, recipient_did, document_uri, created_at, indexed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(uri) DO UPDATE SET
           owner_did = excluded.owner_did,
           recipient_did = excluded.recipient_did,
           document_uri = excluded.document_uri,
           created_at = excluded.created_at,
           indexed_at = excluded.indexed_at",
        params![
            grant.uri,
            grant.owner_did,
            grant.recipient_did,
            grant.document_uri,
            grant.created_at,
            grant.indexed_at,
        ],
    )?;
    Ok(())
}

pub fn delete_grant(conn: &Connection, uri: &str) -> Result<()> {
    conn.execute("DELETE FROM grants WHERE uri = ?1", params![uri])?;
    Ok(())
}

/// Paginated inbox query: grants for a recipient DID, newest first.
/// Cursor is a composite `indexed_at::uri` string.
pub fn list_inbox(
    conn: &Connection,
    recipient_did: &str,
    limit: u32,
    cursor: Option<&str>,
) -> Result<Vec<IndexedGrant>> {
    let mut grants = Vec::new();

    if let Some(cursor) = cursor {
        let (cursor_time, cursor_uri) = parse_cursor(cursor);
        let mut stmt = conn.prepare(
            "SELECT uri, owner_did, recipient_did, document_uri, created_at, indexed_at
             FROM grants
             WHERE recipient_did = ?1
               AND (indexed_at < ?2 OR (indexed_at = ?2 AND uri < ?3))
             ORDER BY indexed_at DESC, uri DESC
             LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![recipient_did, cursor_time, cursor_uri, limit],
            row_to_grant,
        )?;
        for row in rows {
            grants.push(row?);
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT uri, owner_did, recipient_did, document_uri, created_at, indexed_at
             FROM grants
             WHERE recipient_did = ?1
             ORDER BY indexed_at DESC, uri DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![recipient_did, limit], row_to_grant)?;
        for row in rows {
            grants.push(row?);
        }
    }

    Ok(grants)
}

/// List grants created by an owner DID, newest first.
#[allow(dead_code)]
pub fn list_grants_by_owner(
    conn: &Connection,
    owner_did: &str,
    limit: u32,
    cursor: Option<&str>,
) -> Result<Vec<IndexedGrant>> {
    let mut grants = Vec::new();

    if let Some(cursor) = cursor {
        let (cursor_time, cursor_uri) = parse_cursor(cursor);
        let mut stmt = conn.prepare(
            "SELECT uri, owner_did, recipient_did, document_uri, created_at, indexed_at
             FROM grants
             WHERE owner_did = ?1
               AND (indexed_at < ?2 OR (indexed_at = ?2 AND uri < ?3))
             ORDER BY indexed_at DESC, uri DESC
             LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![owner_did, cursor_time, cursor_uri, limit],
            row_to_grant,
        )?;
        for row in rows {
            grants.push(row?);
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT uri, owner_did, recipient_did, document_uri, created_at, indexed_at
             FROM grants
             WHERE owner_did = ?1
             ORDER BY indexed_at DESC, uri DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![owner_did, limit], row_to_grant)?;
        for row in rows {
            grants.push(row?);
        }
    }

    Ok(grants)
}

fn row_to_grant(row: &rusqlite::Row) -> rusqlite::Result<IndexedGrant> {
    Ok(IndexedGrant {
        uri: row.get(0)?,
        owner_did: row.get(1)?,
        recipient_did: row.get(2)?,
        document_uri: row.get(3)?,
        created_at: row.get(4)?,
        indexed_at: row.get(5)?,
    })
}

/// Build a cursor string from an indexed grant.
pub fn encode_cursor(grant: &IndexedGrant) -> String {
    format!("{}::{}", grant.indexed_at, grant.uri)
}

/// Count total grants in the index.
pub fn count_grants(conn: &Connection) -> Result<i64> {
    let count = conn.query_row("SELECT COUNT(*) FROM grants", [], |row| row.get(0))?;
    Ok(count)
}

fn parse_cursor(cursor: &str) -> (&str, &str) {
    match cursor.split_once("::") {
        Some((time, uri)) => (time, uri),
        None => (cursor, ""),
    }
}
