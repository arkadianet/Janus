use rusqlite::Connection;
use std::path::Path;

pub const SCHEMA_SQL: &str = include_str!("schema.sql");

fn io_err(e: std::io::Error) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(e))
}

fn schema_err(msg: String) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(1), Some(msg))
}

pub fn open(path: Option<&Path>) -> rusqlite::Result<Connection> {
    let conn = match path {
        None => Connection::open_in_memory()?,
        Some(p) => {
            if let Some(dir) = p.parent() {
                if !dir.as_os_str().is_empty() {
                    std::fs::create_dir_all(dir).map_err(io_err)?;
                }
            }
            Connection::open(p)?
        }
    };
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    if path.is_some() {
        // WAL / sync / busy_timeout are performance settings.
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
    {
        let tx = conn.unchecked_transaction()?;
        let existing: i64 = tx.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='meta'",
            [],
            |r| r.get(0),
        )?;
        if existing == 0 {
            init_schema(&tx)?;
        } else if !schema_complete(&tx)? {
            recover_incomplete(&tx)?;
        } else {
            migrate(&tx)?;
        }
        tx.commit()?;
    }
    match meta_get(conn, "schema_version")? {
        Some(v) if v == crate::SCHEMA_VERSION => Ok(()),
        other => Err(schema_err(format!(
            "schema_version mismatch: expected {}, found {:?}",
            crate::SCHEMA_VERSION,
            other
        ))),
    }
}

fn table_exists(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [name],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

fn schema_complete(conn: &Connection) -> rusqlite::Result<bool> {
    for t in ["files", "storage_roots", "blobs", "evidence", "file_roles", "model_families"] {
        if !table_exists(conn, t)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn recover_incomplete(conn: &Connection) -> rusqlite::Result<()> {
    if table_exists(conn, "storage_roots")? || table_exists(conn, "files")? {
        return Err(schema_err("incomplete schema: required tables missing".into()));
    }
    conn.execute_batch("DROP TABLE IF EXISTS meta;")?;
    init_schema(conn)
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    if !table_exists(conn, "files")? || !table_exists(conn, "evidence")? || !table_exists(conn, "file_roles")? {
        return recover_incomplete(conn);
    }
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(files)")?;
        let rows = stmt.query_map([], |r| r.get(1))?;
        rows.collect::<Result<_, _>>()?
    };
    if !cols.iter().any(|c| c == "state") {
        conn.execute_batch("ALTER TABLE files ADD COLUMN state TEXT NOT NULL DEFAULT 'present';")?;
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS evidence_subject ON evidence(subject_type, subject_id, field);
         CREATE INDEX IF NOT EXISTS file_roles_variant ON file_roles(variant_id);
         CREATE INDEX IF NOT EXISTS file_roles_family ON file_roles(family_id);",
    )?;
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
    meta_get(conn, "schema_version")?.ok_or_else(|| {
        schema_err("schema_version missing".to_string())
    })
}

pub fn family_key_algo(conn: &Connection) -> rusqlite::Result<String> {
    meta_get(conn, "family_key_algo")?.ok_or_else(|| {
        schema_err("family_key_algo missing".to_string())
    })
}
