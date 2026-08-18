use rusqlite::Connection;
use std::path::Path;

pub const SCHEMA_SQL: &str = include_str!("schema.sql");

pub fn open(path: Option<&Path>) -> rusqlite::Result<Connection> {
    let conn = match path {
        None => Connection::open_in_memory()?,
        Some(p) => {
            if let Some(dir) = p.parent() {
                if !dir.as_os_str().is_empty() {
                    std::fs::create_dir_all(dir).ok();
                }
            }
            Connection::open(p)?
        }
    };
    conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
    if path.is_some() {
        conn.execute_batch("PRAGMA journal_mode = WAL;").ok();
        conn.execute_batch("PRAGMA synchronous = NORMAL;").ok();
        conn.execute_batch("PRAGMA busy_timeout = 5000;").ok();
    }
    Ok(conn)
}

pub fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(())
}

pub fn require_schema(conn: &Connection) -> rusqlite::Result<()> {
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='meta'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()?;
    if tables.is_empty() {
        init_schema(conn)?;
    } else {
        migrate(conn)?;
    }
    Ok(())
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(files)")
        .unwrap()
        .query_map([], |r| r.get(1))
        .unwrap()
        .collect::<Result<_, _>>()?;
    if !cols.iter().any(|c| c == "state") {
        conn.execute_batch("ALTER TABLE files ADD COLUMN state TEXT NOT NULL DEFAULT 'present';")?;
    }
    Ok(())
}

pub fn meta_get(conn: &Connection, k: &str) -> rusqlite::Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT v FROM meta WHERE k = ?1")?;
    let mut rows = stmt.query([k])?;
    match rows.next()? {
        Some(r) => Ok(Some(r.get(0)?)),
        None => Ok(None),
    }
}

pub fn schema_version(conn: &Connection) -> rusqlite::Result<String> {
    Ok(meta_get(conn, "schema_version")?.unwrap_or_else(|| crate::SCHEMA_VERSION.to_string()))
}

pub fn family_key_algo(conn: &Connection) -> rusqlite::Result<String> {
    Ok(meta_get(conn, "family_key_algo")?.unwrap_or_else(|| crate::FAMILY_KEY_ALGO.to_string()))
}