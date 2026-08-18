use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};

pub const ROOT_KINDS: &[&str] = &["internal", "nas", "removable", "discovery", "fetch"];
pub const PRESENT_HYSTERESIS: i64 = 3;

#[derive(Debug, Clone)]
pub struct RootRow {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub kind: String,
    pub mode: String,
    pub present: Option<i64>,
    pub cold: i64,
    pub last_present_check: Option<i64>,
    pub last_scan_at: Option<i64>,
    pub mount_id: Option<String>,
    pub writable: i64,
    pub present_fail_count: i64,
}

#[derive(Debug, Clone, Default)]
pub struct ExistingFile {
    pub id: i64,
    pub size: i64,
    pub mtime: i64,
    pub ctime: i64,
    pub dev: i64,
    pub ino: i64,
    pub blob_id: Option<i64>,
    pub hash_state: Option<String>,
}

pub struct BlobRow {
    pub id: i64,
    pub blake3: String,
    pub sha256: Option<String>,
    pub size: i64,
    pub partial: Option<String>,
}

pub fn root_add(conn: &Connection, name: &str, path: &str, kind: &str) -> Result<i64, String> {
    if !ROOT_KINDS.contains(&kind) {
        return Err(format!("root.bad_kind: {kind}"));
    }
    if kind == "fetch" {
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM storage_roots WHERE kind='fetch'", [], |r| r.get(0))
            .map_err(|e| format!("root.fetch_exists: {e}"))?;
        if n > 0 {
            return Err("root.fetch_exists".to_string());
        }
    }
    let new_path = abs_path(path);
    let stored = new_path.to_string_lossy().into_owned();
    if conn.query_row("SELECT COUNT(*) FROM storage_roots WHERE path=?1", [&stored], |r| r.get::<_, i64>(0)).map_err(to_err)?
        > 0
    {
        return Err("root.duplicate".to_string());
    }
    let mut stmt = conn.prepare("SELECT path FROM storage_roots").map_err(to_err)?;
    let existing_paths = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(to_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_err)?;
    drop(stmt);
    for existing in existing_paths {
        let old_path = abs_path(&existing);
        if new_path.starts_with(&old_path) || old_path.starts_with(&new_path) {
            return Err("root.overlap".to_string());
        }
    }
    let mut kind = kind.to_string();
    if crate::discovery::path_is_discovery(&new_path) && kind != "fetch" {
        kind = "discovery".to_string();
    }
    let mode = if kind == "fetch" { "fetch" } else { "catalogue" };
    let writable = (kind == "fetch") as i64;
    let mount_id = crate::mount::detect_mount_id(&new_path);
    conn.execute(
        "INSERT INTO storage_roots (name, path, kind, mode, writable, mount_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![name, stored, kind, mode, writable, mount_id],
    )
    .map_err(to_err)?;
    Ok(conn.last_insert_rowid())
}

pub fn root_by_id(conn: &Connection, id: i64) -> Result<RootRow, String> {
    let mut stmt = conn
        .prepare("SELECT id,name,path,kind,mode,present,cold,last_present_check,last_scan_at,mount_id,writable,present_fail_count FROM storage_roots WHERE id=?1")
        .map_err(to_err)?;
    let row = stmt
        .query_row([id], root_from_row)
        .map_err(to_err)?;
    Ok(row)
}

fn root_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RootRow> {
    Ok(RootRow {
        id: r.get(0)?,
        name: r.get(1)?,
        path: r.get(2)?,
        kind: r.get(3)?,
        mode: r.get(4)?,
        present: r.get(5)?,
        cold: r.get(6)?,
        last_present_check: r.get(7)?,
        last_scan_at: r.get(8)?,
        mount_id: r.get(9)?,
        writable: r.get(10).unwrap_or(0),
        present_fail_count: r.get(11).unwrap_or(0),
    })
}

pub fn root_ls(conn: &Connection) -> Result<Vec<RootRow>, String> {
    let mut stmt = conn
        .prepare("SELECT id,name,path,kind,mode,present,cold,last_present_check,last_scan_at,mount_id,writable,present_fail_count FROM storage_roots ORDER BY id")
        .map_err(to_err)?;
    let rows = stmt
        .query_map([], |r| root_from_row(r))
        .map_err(to_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_err)?;
    Ok(rows)
}

pub fn root_probe(conn: &Connection, root: &RootRow, now: i64) -> bool {
    if root.cold == 1 {
        return root.present.unwrap_or(0) == 1;
    }
    let alive = Path::new(&root.path).is_dir();
    if alive {
        conn.execute(
            "UPDATE storage_roots SET present=1, present_fail_count=0, last_present_check=?1 WHERE id=?2",
            params![now, root.id],
        )
        .ok();
        return true;
    }
    let fails = root.present_fail_count + 1;
    let present = if fails >= PRESENT_HYSTERESIS { 0 } else { root.present.unwrap_or(1) };
    conn.execute(
        "UPDATE storage_roots SET present=?1, present_fail_count=?2, last_present_check=?3 WHERE id=?4",
        params![present, fails, now, root.id],
    )
    .ok();
    present == 1
}

pub fn root_scan_done(conn: &Connection, root_id: i64, now: i64) {
    conn.execute(
        "UPDATE storage_roots SET last_scan_at=?1 WHERE id=?2",
        params![now, root_id],
    )
    .ok();
}

pub fn file_find(conn: &Connection, root_id: i64, rel: &str) -> Option<ExistingFile> {
    let mut stmt = conn
        .prepare("SELECT id,size,mtime,ctime,dev,ino,blob_id,hash_state FROM files WHERE root_id=?1 AND rel_path=?2")
        .ok()?;
    let mut rows = stmt.query(params![root_id, rel]).ok()?;
    match rows.next().ok()? {
        Some(r) => Some(ExistingFile {
            id: r.get(0).ok()?,
            size: r.get(1).ok()?,
            mtime: r.get(2).ok()?,
            ctime: r.get(3).ok()?,
            dev: r.get(4).ok()?,
            ino: r.get(5).ok()?,
            blob_id: r.get(6).ok()?,
            hash_state: r.get(7).ok()?,
        }),
        None => None,
    }
}

pub fn blob_find(conn: &Connection, id: i64) -> Option<BlobRow> {
    let mut stmt = conn
        .prepare("SELECT id,blake3,sha256,size,xxhash64_partial FROM blobs WHERE id=?1")
        .ok()?;
    let mut rows = stmt.query([id]).ok()?;
    match rows.next().ok()? {
        Some(r) => Some(BlobRow {
            id: r.get(0).ok()?,
            blake3: r.get(1).ok()?,
            sha256: r.get(2).ok()?,
            size: r.get(3).ok()?,
            partial: r.get(4).ok()?,
        }),
        None => None,
    }
}

pub fn blob_upsert(
    conn: &Connection,
    blake3: &str,
    sha256: Option<&str>,
    size: i64,
    partial: Option<&str>,
) -> Result<i64, String> {
    let existing: Option<i64> = match conn.query_row("SELECT id FROM blobs WHERE blake3=?1", [blake3], |r| r.get(0)) {
        Ok(id) => Some(id),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(to_err(e)),
    };
    match existing {
        Some(id) => {
            conn.execute(
                "UPDATE blobs SET size=?1,
                                  sha256=COALESCE(?2, sha256),
                                  xxhash64_partial=COALESCE(?3, xxhash64_partial)
                 WHERE id=?4",
                params![size, sha256, partial, id],
            )
            .map_err(to_err)?;
            Ok(id)
        }
        None => {
            conn.execute(
                "INSERT INTO blobs (blake3, sha256, size, refcount, xxhash64_partial) VALUES (?1,?2,?3,1,?4)",
                params![blake3, sha256, size, partial],
            )
            .map_err(to_err)?;
            Ok(conn.last_insert_rowid())
        }
    }
}

pub fn file_upsert(
    conn: &Connection,
    root_id: i64,
    rel: &str,
    size: i64,
    mtime: i64,
    ctime: i64,
    dev: i64,
    ino: i64,
    regular: bool,
    symlink_target: Option<&str>,
    blob_id: Option<i64>,
    hash_state: &str,
    parse_state: &str,
    parse_error: Option<&str>,
) -> Result<i64, String> {
    let _updated = if symlink_target.is_some() {
        conn.execute(
            "UPDATE files SET size=?1,mtime=?2,ctime=?3,dev=?4,ino=?5,is_symlink=1,symlink_target=?11,blob_id=?6,hash_state=?7,parse_state=?8,parse_error=?9,state='present' WHERE root_id=?10 AND rel_path=?12",
            params![size, mtime, ctime, dev, ino, blob_id, hash_state, parse_state, parse_error, root_id, symlink_target, rel],
        )
    } else {
        conn.execute(
            "UPDATE files SET size=?1,mtime=?2,ctime=?3,dev=?4,ino=?5,is_symlink=?6,symlink_target=NULL,blob_id=?7,hash_state=?8,parse_state=?9,parse_error=?10,state='present' WHERE root_id=?11 AND rel_path=?12",
            params![size, mtime, ctime, dev, ino, !regular as i64, blob_id, hash_state, parse_state, parse_error, root_id, rel],
        )
    }
    .map_err(to_err)?;
    if _updated > 0 {
        return Ok(conn
            .query_row(
                "SELECT id FROM files WHERE root_id=?1 AND rel_path=?2",
                params![root_id, rel],
                |r| r.get(0),
            )
            .map_err(to_err)?);
    }
    conn.execute(
        "INSERT INTO files (root_id, rel_path, size, mtime, ctime, dev, ino, is_symlink, symlink_target, blob_id, hash_state, parse_state, parse_error, state) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,'present')",
        params![root_id, rel, size, mtime, ctime, dev, ino, !regular as i64, symlink_target, blob_id, hash_state, parse_state, parse_error],
    )
    .map_err(to_err)?;
    Ok(conn.last_insert_rowid())
}

pub fn family_find(conn: &Connection, key: &str) -> Option<i64> {
    conn.query_row("SELECT id FROM model_families WHERE family_key=?1", [key], |r| r.get(0)).ok()
}

pub fn family_resolve(conn: &Connection, key: &str) -> Option<i64> {
    if let Some(id) = family_find(conn, key) {
        return Some(id);
    }
    conn.query_row("SELECT family_id FROM family_aliases WHERE alias=?1", [key], |r| r.get(0))
        .ok()
}

pub fn family_key_of(conn: &Connection, id: i64) -> Option<String> {
    conn.query_row("SELECT family_key FROM model_families WHERE id=?1", [id], |r| r.get(0))
        .ok()
}

pub fn merge_families(conn: &Connection, src: &str, target: &str) -> Result<i64, String> {
    let src_id = family_find_id(conn, src).ok_or_else(|| "identity.not_found".to_string())?;
    let target_id = family_find_id(conn, target).ok_or_else(|| "identity.not_found".to_string())?;
    if src_id == target_id {
        return Ok(target_id);
    }
    let src_key = family_key_of(conn, src_id).ok_or_else(|| "identity.not_found".to_string())?;
    let target_key = family_key_of(conn, target_id).ok_or_else(|| "identity.not_found".to_string())?;
    let src_name: Option<String> = conn
        .query_row("SELECT name FROM model_families WHERE id=?1", [src_id], |r| r.get(0))
        .ok();
    if is_declined(conn, &src_key, &target_key, crate::FAMILY_KEY_ALGO) {
        return Err("identity.merge_declined".to_string());
    }

    reassign_revisions(conn, src_id, target_id)?;
    reassign_variants(conn, src_id, target_id)?;
    conn.execute(
        "UPDATE file_roles SET family_id=?1 WHERE family_id=?2",
        params![target_id, src_id],
    )
    .map_err(to_err)?;
    conn.execute(
        "UPDATE evidence SET subject_id=?1 WHERE subject_type='family' AND subject_id=?2",
        params![target_id, src_id],
    )
    .map_err(to_err)?;
    conn.execute(
        "INSERT OR IGNORE INTO family_aliases (family_id, alias, source) VALUES (?1, ?2, 'manual')",
        params![target_id, src_key],
    )
    .map_err(to_err)?;
    if let Some(name) = src_name {
        if name != src_key {
            conn.execute(
                "INSERT OR IGNORE INTO family_aliases (family_id, alias, source) VALUES (?1, ?2, 'manual')",
                params![target_id, name],
            )
            .map_err(to_err)?;
        }
    }
    conn.execute("DELETE FROM model_families WHERE id=?1", [src_id])
        .map_err(to_err)?;
    Ok(target_id)
}

fn reassign_revisions(conn: &Connection, src_id: i64, target_id: i64) -> Result<(), String> {
    let mut stmt = conn
        .prepare("SELECT id, rev_kind, rev_label FROM model_revisions WHERE family_id=?1")
        .map_err(to_err)?;
    let rows = stmt
        .query_map([src_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })
        .map_err(to_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_err)?;
    drop(stmt);
    for (rev_id, kind, label) in rows {
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM model_revisions WHERE family_id=?1 AND rev_kind=?2 AND rev_label=?3",
                params![target_id, kind, label],
                |r| r.get(0),
            )
            .ok();
        if let Some(keep) = existing {
            conn.execute(
                "UPDATE model_variants SET revision_id=?1 WHERE revision_id=?2",
                params![keep, rev_id],
            )
            .map_err(to_err)?;
            conn.execute("DELETE FROM model_revisions WHERE id=?1", [rev_id])
                .map_err(to_err)?;
        } else {
            conn.execute(
                "UPDATE model_revisions SET family_id=?1 WHERE id=?2",
                params![target_id, rev_id],
            )
            .map_err(to_err)?;
        }
    }
    Ok(())
}

fn reassign_variants(conn: &Connection, src_id: i64, target_id: i64) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, revision_id, quant, quant_raw, format, subflavour, publisher
             FROM model_variants WHERE family_id=?1",
        )
        .map_err(to_err)?;
    let rows = stmt
        .query_map([src_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })
        .map_err(to_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_err)?;
    drop(stmt);
    for (vid, revision_id, quant, _quant_raw, format, subflavour, publisher) in rows {
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM model_variants
                 WHERE family_id=?1 AND revision_id=?2 AND format=?3 AND quant=?4
                   AND subflavour=?5 AND publisher=?6",
                params![target_id, revision_id, format, quant, subflavour, publisher],
                |r| r.get(0),
            )
            .ok();
        if let Some(keep) = existing {
            conn.execute(
                "UPDATE file_roles SET variant_id=?1 WHERE variant_id=?2",
                params![keep, vid],
            )
            .map_err(to_err)?;
            conn.execute("DELETE FROM model_variants WHERE id=?1", [vid])
                .map_err(to_err)?;
        } else {
            conn.execute(
                "UPDATE model_variants SET family_id=?1 WHERE id=?2",
                params![target_id, vid],
            )
            .map_err(to_err)?;
        }
    }
    Ok(())
}

pub fn family_insert(
    conn: &Connection,
    key: &str,
    name: Option<&str>,
    arch: Option<&str>,
    params_total: Option<f64>,
    params_active: Option<f64>,
    context_len: Option<i64>,
    kind: &str,
) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO model_families (family_key, name, arch, params_total, params_active, context_len, kind) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![key, name, arch, params_total, params_active, context_len, kind],
    )
    .map_err(to_err)?;
    Ok(conn.last_insert_rowid())
}

pub fn revision_find_or_insert(conn: &Connection, family_id: i64, rev_label: &str) -> Result<i64, String> {
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM model_revisions WHERE family_id=?1 AND rev_kind='local' AND rev_label=?2",
            params![family_id, rev_label],
            |r| r.get(0),
        )
        .ok()
    {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO model_revisions (family_id, rev_kind, rev_label) VALUES (?1,'local',?2)",
        params![family_id, rev_label],
    )
    .map_err(to_err)?;
    Ok(conn.last_insert_rowid())
}

pub fn variant_find_or_insert(
    conn: &Connection,
    family_id: i64,
    revision_id: i64,
    quant: &str,
    quant_raw: Option<&str>,
    format: &str,
    subflavour: &str,
    publisher: &str,
) -> Result<i64, String> {
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM model_variants WHERE family_id=?1 AND revision_id=?2 AND format=?3 AND quant=?4 AND subflavour=?5 AND publisher=?6",
            params![family_id, revision_id, format, quant, subflavour, publisher],
            |r| r.get(0),
        )
        .ok()
    {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO model_variants (family_id, revision_id, quant, quant_raw, format, subflavour, publisher) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![family_id, revision_id, quant, quant_raw, format, subflavour, publisher],
    )
    .map_err(to_err)?;
    Ok(conn.last_insert_rowid())
}

pub fn file_role_put(conn: &Connection, file_id: i64, variant_id: Option<i64>, family_id: Option<i64>, role: &str) {
    conn.execute(
        "INSERT OR REPLACE INTO file_roles (file_id, variant_id, family_id, role) VALUES (?1,?2,?3,?4)",
        params![file_id, variant_id, family_id, role],
    )
    .ok();
}

pub fn evidence_put(conn: &Connection, subject_type: &str, subject_id: i64, field: &str, value: &str, level: &str, source: &str) {
    if value.is_empty() {
        return;
    }
    conn.execute(
        "INSERT INTO evidence (subject_type, subject_id, field, value, level, source, recorded_at) VALUES (?1,?2,?3,?4,?5,?6, strftime('%s','now'))",
        params![subject_type, subject_id, field, value, level, source],
    )
    .ok();
}

pub fn declined_merge(conn: &Connection, family_a_key: &str, family_b_key: &str, algo_version: &str) -> Result<(), String> {
    let (a, b) = if family_a_key < family_b_key {
        (family_a_key, family_b_key)
    } else {
        (family_b_key, family_a_key)
    };
    conn.execute(
        "INSERT OR IGNORE INTO declined_merges (family_a_key, family_b_key, algo_version, declined_at) VALUES (?1,?2,?3, strftime('%s','now'))",
        params![a, b, algo_version],
    )
    .map_err(to_err)?;
    Ok(())
}

pub fn is_declined(conn: &Connection, family_a_key: &str, family_b_key: &str, algo_version: &str) -> bool {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM declined_merges
             WHERE algo_version=?1 AND (
                (family_a_key=?2 AND family_b_key=?3) OR (family_a_key=?3 AND family_b_key=?2)
             )",
            params![algo_version, family_a_key, family_b_key],
            |r| r.get(0),
        )
        .unwrap_or(0);
    n > 0
}

pub fn is_aliased(conn: &Connection, family_a_key: &str, family_b_key: &str) -> bool {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM family_aliases a
             JOIN model_families f ON f.id=a.family_id
             WHERE (a.alias=?1 AND f.family_key=?2) OR (a.alias=?2 AND f.family_key=?1)",
            params![family_a_key, family_b_key],
            |r| r.get(0),
        )
        .unwrap_or(0);
    n > 0
}

fn abs_path(p: &str) -> PathBuf {
    let p = PathBuf::from(p);
    std::fs::canonicalize(&p).unwrap_or_else(|_| {
        if p.is_absolute() {
            p
        } else {
            std::env::current_dir().map(|c| c.join(&p)).unwrap_or(p)
        }
    })
}

pub fn to_err(e: rusqlite::Error) -> String {
    format!("db:{e}")
}

pub fn present_count(conn: &Connection) -> Result<(i64, i64), String> {
    let all: i64 = conn
        .query_row("SELECT COUNT(*) FROM storage_roots", [], |r| r.get(0))
        .map_err(to_err)?;
    let present: i64 = conn
        .query_row("SELECT COUNT(*) FROM storage_roots WHERE present=1", [], |r| r.get(0))
        .map_err(to_err)?;
    Ok((all, present))
}

#[derive(Debug, Clone)]
pub struct ListFamily {
    pub id: i64,
    pub key: String,
    pub name: Option<String>,
    pub name_level: Option<String>,
    pub kind: String,
    pub params_total: Option<f64>,
    pub params_active: Option<f64>,
    pub quants: String,
    pub bytes: i64,
    pub roots: Vec<(String, bool)>,
}

pub fn family_list(conn: &Connection) -> Result<Vec<ListFamily>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT f.id, f.family_key, f.name, f.kind, f.params_total, f.params_active,
               (SELECT e.level FROM evidence e WHERE e.subject_type='family' AND e.subject_id=f.id
                AND e.field='name' ORDER BY e.id DESC LIMIT 1) AS name_level,
               (SELECT COALESCE(SUM(fl.size),0)
                  FROM files fl JOIN file_roles fr ON fr.file_id=fl.id
                  LEFT JOIN model_variants v ON v.id=fr.variant_id
                 WHERE v.family_id=f.id OR fr.family_id=f.id) AS bytes
             FROM model_families f ORDER BY COALESCE(f.name, f.family_key)",
        )
        .map_err(to_err)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<f64>>(4)?,
                r.get::<_, Option<f64>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, i64>(7)?,
            ))
        })
        .map_err(to_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_err)?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, key, name, kind, total, active, name_level, bytes) in rows {
        let q: Option<String> = conn
            .query_row(
                "SELECT GROUP_CONCAT(DISTINCT v.quant) FROM model_variants v WHERE v.family_id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(to_err)?;
        let mut roots: Vec<(String, bool)> = Vec::new();
        if let Ok(mut rs) = conn.prepare(
            "SELECT DISTINCT sr.name, COALESCE(sr.present,0)
               FROM storage_roots sr
               JOIN files fl ON fl.root_id=sr.id
               JOIN file_roles fr ON fr.file_id=fl.id
               LEFT JOIN model_variants v ON v.id=fr.variant_id
              WHERE v.family_id=?1 OR fr.family_id=?1
              ORDER BY sr.name",
        ) {
            let it = rs
                .query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? == 1)))
                .map_err(to_err)?;
            for r in it.flatten() {
                roots.push(r);
            }
        }
        out.push(ListFamily {
            id,
            key,
            name,
            name_level,
            kind,
            params_total: total,
            params_active: active,
            quants: q.unwrap_or_default(),
            bytes,
            roots,
        });
    }
    Ok(out)
}

pub fn family_find_id(conn: &Connection, name_or_key: &str) -> Option<i64> {
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM model_families WHERE family_key=?1 OR name=?1 LIMIT 1",
            [name_or_key],
            |r| r.get(0),
        )
        .ok()
    {
        return Some(id);
    }
    family_resolve(conn, name_or_key)
}

#[derive(Debug, Clone)]
pub struct ShowVariant {
    pub quant: String,
    pub format: String,
    pub subflavour: String,
    pub publisher: String,
    pub bytes: i64,
    pub root: String,
    pub present: bool,
    pub last_file_mtime: Option<i64>,
}

pub fn family_variants(conn: &Connection, family_id: i64) -> Result<Vec<ShowVariant>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT v.quant, v.format, v.subflavour, v.publisher,
               (SELECT COALESCE(SUM(fl.size),0)
                  FROM files fl JOIN file_roles fr ON fr.file_id=fl.id
                 WHERE fr.variant_id=v.id) AS bytes,
               (SELECT sr.name FROM storage_roots sr
                 JOIN files fl ON fl.root_id=sr.id
                 JOIN file_roles fr ON fr.file_id=fl.id
                WHERE fr.variant_id=v.id LIMIT 1) AS root,
               (SELECT COALESCE(sr.present,0) FROM storage_roots sr
                 JOIN files fl ON fl.root_id=sr.id
                 JOIN file_roles fr ON fr.file_id=fl.id
                WHERE fr.variant_id=v.id LIMIT 1) AS present,
               (SELECT MAX(fl.mtime) FROM file_roles fr JOIN files fl ON fl.id=fr.file_id
                WHERE fr.variant_id=v.id) AS last_file_mtime
             FROM model_variants v WHERE v.family_id=?1 ORDER BY v.quant",
        )
        .map_err(to_err)?;
    let rows = stmt
        .query_map([family_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?, r.get::<_, Option<String>>(5)?, r.get::<_, i64>(6)?, r.get::<_, Option<i64>>(7)?))
        })
        .map_err(to_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_err)?;
    Ok(rows
        .into_iter()
        .map(|(quant, format, subflavour, publisher, bytes, root, present, last_file_mtime)| ShowVariant {
            quant,
            format,
            subflavour,
            publisher,
            bytes,
            root: root.unwrap_or_default(),
            present: present == 1,
            last_file_mtime,
        })
        .collect())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StorageRow {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub present: bool,
    pub cold: bool,
    pub files: i64,
    pub bytes: i64,
    pub reclaimable: i64,
}

pub fn storage_summary(conn: &Connection) -> Result<Vec<StorageRow>, String> {
    let roots = root_ls(conn)?;
    let groups = crate::dedup::plan(conn);
    let mut out = Vec::with_capacity(roots.len());
    for r in roots {
        let files: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE root_id=?1 AND state='present'",
                [r.id],
                |row| row.get(0),
            )
            .map_err(to_err)?;
        let bytes: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(size),0) FROM files WHERE root_id=?1 AND state='present'",
                [r.id],
                |row| row.get(0),
            )
            .map_err(to_err)?;
        let reclaimable: i64 = if r.present.unwrap_or(0) == 1 {
            groups
                .iter()
                .map(|g| {
                    let inodes: std::collections::HashSet<(i64, i64)> = g
                        .copies
                        .iter()
                        .filter(|c| c.root_id == r.id)
                        .map(|c| (c.dev, c.ino))
                        .collect();
                    if inodes.is_empty() {
                        0
                    } else {
                        (inodes.len() as i64).saturating_sub(1) * g.size
                    }
                })
                .sum()
        } else {
            0
        };
        out.push(StorageRow {
            id: r.id,
            name: r.name,
            kind: r.kind,
            present: r.present.unwrap_or(0) == 1,
            cold: r.cold == 1,
            files,
            bytes,
            reclaimable,
        });
    }
    Ok(out)
}

pub fn root_rm(conn: &Connection, id: i64) -> Result<(), String> {
    let tx = conn.unchecked_transaction().map_err(to_err)?;
    tx.execute(
        "DELETE FROM file_roles WHERE file_id IN (SELECT id FROM files WHERE root_id=?1)",
        [id],
    )
    .map_err(to_err)?;
    tx.execute("DELETE FROM files WHERE root_id=?1", [id]).map_err(to_err)?;
    let n = tx.execute("DELETE FROM storage_roots WHERE id=?1", [id]).map_err(to_err)?;
    if n == 0 {
        return Err("root.not_found".into());
    }
    tx.commit().map_err(to_err)?;
    Ok(())
}

pub fn persist_manual_name_id(conn: &Connection, file_id: i64, name: &str) -> Result<i64, String> {
    let (root_path, rel): (String, String) = conn
        .query_row(
            "SELECT r.path, f.rel_path FROM files f JOIN storage_roots r ON r.id=f.root_id WHERE f.id=?1",
            [file_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "identity.not_found".to_string())?;
    let path = PathBuf::from(root_path).join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    persist_manual_name(conn, &path, name)
}

pub fn job_insert(conn: &Connection, kind: &str) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO jobs (kind, state, progress, started) VALUES (?1, 'running', 0, strftime('%s','now'))",
        [kind],
    )
    .map_err(to_err)?;
    Ok(conn.last_insert_rowid())
}

pub fn job_finish(conn: &Connection, id: i64, state: &str, progress: f64, error: Option<&str>) -> Result<(), String> {
    conn.execute(
        "UPDATE jobs SET state=?1, progress=?2, finished=strftime('%s','now'), error_json=?3 WHERE id=?4",
        params![state, progress, error, id],
    )
    .map_err(to_err)?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct JobRow {
    pub id: i64,
    pub kind: String,
    pub state: String,
    pub progress: Option<f64>,
    pub started: Option<i64>,
    pub finished: Option<i64>,
    pub error: Option<String>,
}

pub fn job_get(conn: &Connection, id: i64) -> Result<JobRow, String> {
    conn.query_row(
        "SELECT id, COALESCE(kind,''), COALESCE(state,''), progress, started, finished, error_json FROM jobs WHERE id=?1",
        [id],
        |r| {
            Ok(JobRow {
                id: r.get(0)?,
                kind: r.get(1)?,
                state: r.get(2)?,
                progress: r.get(3)?,
                started: r.get(4)?,
                finished: r.get(5)?,
                error: r.get(6)?,
            })
        },
    )
    .map_err(|_| "identity.not_found".to_string())
}

pub fn job_list(conn: &Connection, limit: usize) -> Result<Vec<JobRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, COALESCE(kind,''), COALESCE(state,''), progress, started, finished, error_json
             FROM jobs ORDER BY id DESC LIMIT ?1",
        )
        .map_err(to_err)?;
    let rows = stmt
        .query_map([limit as i64], |r| {
            Ok(JobRow {
                id: r.get(0)?,
                kind: r.get(1)?,
                state: r.get(2)?,
                progress: r.get(3)?,
                started: r.get(4)?,
                finished: r.get(5)?,
                error: r.get(6)?,
            })
        })
        .map_err(to_err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(to_err)
}

pub fn file_abs_path(conn: &Connection, file_id: i64) -> Result<PathBuf, String> {
    let (root_path, rel): (String, String) = conn
        .query_row(
            "SELECT r.path, f.rel_path FROM files f JOIN storage_roots r ON r.id=f.root_id WHERE f.id=?1",
            [file_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "identity.not_found".to_string())?;
    Ok(PathBuf::from(root_path).join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)))
}

pub fn root_set_cold(conn: &Connection, id: i64, cold: bool) -> Result<(), String> {
    let n = conn
        .execute("UPDATE storage_roots SET cold=?1 WHERE id=?2", params![cold as i64, id])
        .map_err(to_err)?;
    if n == 0 {
        return Err("root.not_found".to_string());
    }
    Ok(())
}

pub fn home_counts(conn: &Connection) -> Result<(i64, i64, i64, i64, i64), String> {
    let families: i64 = conn.query_row("SELECT COUNT(*) FROM model_families", [], |r| r.get(0)).map_err(to_err)?;
    let families_inferred: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM model_families f WHERE NOT EXISTS (SELECT 1 FROM evidence e WHERE e.subject_type='family' AND e.subject_id=f.id AND e.level IN ('known','manual') AND e.field='name')",
            [],
            |r| r.get(0),
        )
        .map_err(to_err)?;
    let bytes: i64 = conn
        .query_row("SELECT COALESCE(SUM(size),0) FROM files", [], |r| r.get(0))
        .map_err(to_err)?;
    let unverified: i64 = conn
        .query_row("SELECT COUNT(*) FROM files WHERE hash_state != 'full'", [], |r| r.get(0))
        .map_err(to_err)?;
    let unknown_files: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM files f LEFT JOIN file_roles r ON r.file_id=f.id WHERE r.file_id IS NULL AND f.parse_state='ok'",
            [],
            |r| r.get(0),
        )
        .map_err(to_err)?;
    Ok((families, families_inferred, bytes, unverified, unknown_files))
}

pub fn family_for_blob(conn: &Connection, blob_id: i64) -> Option<i64> {
    conn.query_row(
        "SELECT COALESCE(v.family_id, fr.family_id) FROM file_roles fr
         JOIN files f ON f.id=fr.file_id
         LEFT JOIN model_variants v ON v.id=fr.variant_id
         WHERE f.blob_id=?1 AND (v.family_id IS NOT NULL OR fr.family_id IS NOT NULL)
         LIMIT 1",
        [blob_id],
        |r| r.get::<_, Option<i64>>(0),
    )
    .ok()
    .flatten()
}

pub fn provenance_put(
    conn: &Connection,
    subject_type: &str,
    subject_id: i64,
    event: &str,
    source_kind: &str,
    repo: Option<&str>,
    revision: Option<&str>,
) {
    conn.execute(
        "INSERT INTO provenance_entries (subject_type, subject_id, event, source_kind, repo, revision, at)
         VALUES (?1,?2,?3,?4,?5,?6, strftime('%s','now'))",
        params![subject_type, subject_id, event, source_kind, repo, revision],
    )
    .ok();
}

pub fn root_containing(conn: &Connection, path: &Path) -> Option<RootRow> {
    let abs = abs_path(&path.to_string_lossy());
    for r in root_ls(conn).ok()? {
        if abs.starts_with(Path::new(&r.path)) {
            return Some(r);
        }
    }
    None
}

pub fn persist_manual_name(conn: &Connection, path: &Path, name: &str) -> Result<i64, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("identity.not_found".into());
    }
    let root = root_containing(conn, path).ok_or_else(|| "root.not_found".to_string())?;
    let abs = abs_path(&path.to_string_lossy());
    let rel = abs
        .strip_prefix(&root.path)
        .map_err(|_| "root.not_found".to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    let file = file_find(conn, root.id, &rel).ok_or_else(|| "identity.not_found".to_string())?;
    let key = crate::identity::family_key(name, None, None, None);
    let family_id = match family_resolve(conn, &key) {
        Some(id) => id,
        None => family_insert(conn, &key, Some(name), None, None, None, None, "unknown")?,
    };
    evidence_put(conn, "family", family_id, "name", name, "manual", "user");
    let rev = revision_find_or_insert(conn, family_id, "local:none")?;
    let vid = variant_find_or_insert(conn, family_id, rev, "unknown", None, "unknown", "unknown", "unknown")?;
    file_role_put(conn, file.id, Some(vid), Some(family_id), "weights");
    Ok(family_id)
}

pub fn discover_roots(conn: &Connection) -> Result<Vec<i64>, String> {
    let mut ids = Vec::new();
    for h in crate::discovery::candidates() {
        match root_add(conn, &h.name, h.path.to_string_lossy().as_ref(), "discovery") {
            Ok(id) => ids.push(id),
            Err(e) if e == "root.overlap" || e == "root.duplicate" => {}
            Err(e) => return Err(e),
        }
    }
    Ok(ids)
}

pub fn db_path() -> PathBuf {
    dirs::data_dir()
        .map(|d| d.join("janus").join("janus.db"))
        .unwrap_or_else(|| PathBuf::from("janus.db"))
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .map(|d| d.join("janus"))
        .unwrap_or_else(|| PathBuf::from(".cache/janus"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn mem() -> Connection {
        let c = db::open(None).unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    fn seed_pair(c: &Connection) -> (i64, i64) {
        let a = family_insert(c, "foo|llama|t8|a8", Some("Foo"), Some("llama"), Some(8.0), Some(8.0), None, "llm").unwrap();
        let b = family_insert(c, "bar|llama|t8|a8", Some("Bar"), Some("llama"), Some(8.0), Some(8.0), None, "llm").unwrap();
        (a, b)
    }

    #[test]
    fn merge_writes_alias_and_drops_src_family() {
        let c = mem();
        let (_a, b) = seed_pair(&c);
        let got = merge_families(&c, "Foo", "Bar").unwrap();
        assert_eq!(got, b);
        let n: i64 = c.query_row("SELECT COUNT(*) FROM model_families", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        let aliases: i64 = c.query_row("SELECT COUNT(*) FROM family_aliases", [], |r| r.get(0)).unwrap();
        assert_eq!(aliases, 2);
        assert_eq!(family_resolve(&c, "foo|llama|t8|a8"), Some(b));
        assert_eq!(family_find_id(&c, "foo|llama|t8|a8"), Some(b));
        assert_eq!(family_find_id(&c, "Foo"), Some(b));
    }

    #[test]
    fn merge_refuses_declined_pair() {
        let c = mem();
        seed_pair(&c);
        declined_merge(&c, "foo|llama|t8|a8", "bar|llama|t8|a8", crate::FAMILY_KEY_ALGO).unwrap();
        let err = merge_families(&c, "Foo", "Bar").unwrap_err();
        assert_eq!(err, "identity.merge_declined");
    }

    #[test]
    fn storage_summary_lists_roots_and_cold_flag() {
        let c = mem();
        let dir = std::env::temp_dir().join(format!("janus-storage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let id = root_add(&c, "models", dir.to_str().unwrap(), "internal").unwrap();
        root_set_cold(&c, id, true).unwrap();
        root_probe(&c, &root_by_id(&c, id).unwrap(), 1);
        let rows = storage_summary(&c).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "models");
        assert!(rows[0].cold);
        assert_eq!(rows[0].reclaimable, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cold_unknown_root_is_not_found() {
        let c = mem();
        assert_eq!(root_set_cold(&c, 99, true).unwrap_err(), "root.not_found");
    }

    #[test]
    fn hysteresis_needs_n_failures() {
        let c = mem();
        let dir = std::env::temp_dir().join(format!("janus-hyst-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let id = root_add(&c, "drawer", dir.to_str().unwrap(), "removable").unwrap();
        let r = root_by_id(&c, id).unwrap();
        assert!(root_probe(&c, &r, 1));
        let _ = std::fs::remove_dir_all(&dir);
        for i in 0..PRESENT_HYSTERESIS - 1 {
            let r = root_by_id(&c, id).unwrap();
            assert!(root_probe(&c, &r, 10 + i), "still present after fail {}", i + 1);
        }
        let r = root_by_id(&c, id).unwrap();
        assert!(!root_probe(&c, &r, 99));
        assert_eq!(root_by_id(&c, id).unwrap().present, Some(0));
    }

    #[test]
    fn identify_persists_manual_name_under_root() {
        let c = mem();
        let dir = std::env::temp_dir().join(format!("janus-id-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let id = root_add(&c, "models", dir.to_str().unwrap(), "internal").unwrap();
        let path = dir.join("random.safetensors");
        std::fs::write(&path, b"not-a-real-st").unwrap();
        c.execute(
            "INSERT INTO files (root_id, rel_path, size, mtime, ctime, dev, ino, is_symlink, hash_state, parse_state, state)
             VALUES (?1,'random.safetensors',13,0,0,0,1,0,'none','ok','present')",
            [id],
        )
        .unwrap();
        let fid = persist_manual_name(&c, &path, "MyModel").unwrap();
        let name: String = c.query_row("SELECT name FROM model_families WHERE id=?1", [fid], |r| r.get(0)).unwrap();
        assert_eq!(name, "MyModel");
        let level: String = c
            .query_row(
                "SELECT level FROM evidence WHERE subject_type='family' AND subject_id=?1 AND field='name'",
                [fid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(level, "manual");
        let _ = std::fs::remove_dir_all(&dir);
    }
}