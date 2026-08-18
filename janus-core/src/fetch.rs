//! Fetch one wanted item into the fetch root. Never writes discovery roots.

use rusqlite::{params, Connection};
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::path::{Component, Path, PathBuf};

use crate::discovery;
use crate::hash;
use crate::radar::{self, WantedRow};
use crate::scan::{self, ScanOptions};
use crate::store;

#[derive(Debug, Clone, Serialize)]
pub struct FetchResult {
    pub task_id: i64,
    pub wanted_id: i64,
    pub state: String,
    pub dest: String,
}

pub trait ByteSource {
    fn download(&self, wanted: &WantedRow, dest: &Path) -> Result<(), String>;
}

pub struct MemoryBytes(pub Vec<u8>);

impl ByteSource for MemoryBytes {
    fn download(&self, _wanted: &WantedRow, dest: &Path) -> Result<(), String> {
        if let Some(p) = dest.parent() {
            fs::create_dir_all(p).map_err(|e| format!("scan.io: {e}"))?;
        }
        fs::write(dest, &self.0).map_err(|e| format!("scan.io: {e}"))
    }
}

pub struct HfHttps;

impl ByteSource for HfHttps {
    fn download(&self, wanted: &WantedRow, dest: &Path) -> Result<(), String> {
        let url = format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            wanted.repo, wanted.revision, wanted.filename
        );
        if let Some(p) = dest.parent() {
            fs::create_dir_all(p).map_err(|e| format!("scan.io: {e}"))?;
        }
        let token = std::env::var("HF_TOKEN").ok().filter(|s| !s.is_empty());
        let mut req = ureq::get(&url);
        if let Some(t) = &token {
            req = req.set("Authorization", &format!("Bearer {t}"));
        }
        let resp = req.call().map_err(|e| format!("network.disabled: {e}"))?;
        let mut file = File::create(dest).map_err(|e| format!("scan.io: {e}"))?;
        let mut reader = resp.into_reader();
        std::io::copy(&mut reader, &mut file).map_err(|e| format!("scan.io: {e}"))?;
        file.sync_all().map_err(|e| format!("scan.io: {e}"))?;
        Ok(())
    }
}

pub fn validate_dest_rel(fetch_root: &Path, dest_rel: &str) -> Result<PathBuf, String> {
    let raw = dest_rel.trim();
    if raw.is_empty() {
        return Err("fetch.path_invalid".into());
    }
    if raw.contains('\\') && raw.contains(':') {
        return Err("fetch.path_invalid".into());
    }
    if raw.starts_with("\\\\") || raw.starts_with("//") {
        return Err("fetch.path_invalid".into());
    }
    let p = Path::new(raw);
    if p.is_absolute() {
        return Err("fetch.path_invalid".into());
    }
    let mut out = fetch_root.to_path_buf();
    for c in p.components() {
        match c {
            Component::Normal(s) => {
                let t = s.to_string_lossy();
                if t.contains(':') {
                    return Err("fetch.path_invalid".into());
                }
                out.push(s);
            }
            Component::CurDir => {}
            _ => return Err("fetch.path_invalid".into()),
        }
    }
    if !out.starts_with(fetch_root) {
        return Err("fetch.path_invalid".into());
    }
    if let Some(parent) = out.parent() {
        if parent.is_symlink() {
            return Err("fetch.path_invalid".into());
        }
    }
    if discovery::path_is_discovery(&out) {
        return Err("root.discovery_readonly".into());
    }
    Ok(out)
}

pub fn fetch_root(conn: &Connection) -> Result<store::RootRow, String> {
    store::root_ls(conn)?
        .into_iter()
        .find(|r| r.kind == "fetch")
        .ok_or_else(|| "root.not_writable".to_string())
}

pub fn fetch_wanted(
    conn: &Connection,
    wanted_id: i64,
    dest_rel: Option<&str>,
    force: bool,
    source: &dyn ByteSource,
) -> Result<FetchResult, String> {
    let wanted = radar::wanted_by_id(conn, wanted_id)?;
    let root = fetch_root(conn)?;
    if root.kind != "fetch" || root.writable != 1 {
        return Err("root.not_writable".into());
    }
    if discovery::path_is_discovery(Path::new(&root.path)) {
        return Err("root.discovery_readonly".into());
    }
    let rel = dest_rel
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| Path::new(&wanted.filename).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| wanted.filename.clone()));
    let dest = validate_dest_rel(Path::new(&root.path), &rel)?;
    let sha = wanted.sha256.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let task_id = task_upsert(conn, wanted_id, root.id, &rel, "queued")?;

    if sha.is_none() {
        task_error(conn, task_id, "wanted.no_sha256")?;
        return Err("wanted.no_sha256".into());
    }
    let sha = sha.unwrap().trim_start_matches("sha256:").to_ascii_lowercase();

    if !force {
        if let Some((_, root_id)) = radar::have_verified_sha256(conn, &sha) {
            let owned = store::root_by_id(conn, root_id).ok();
            let note = owned
                .map(|r| format!("owned on {}{}", r.name, if r.present.unwrap_or(0) == 1 { "" } else { " (offline)" }))
                .unwrap_or_else(|| "already owned".into());
            task_error(conn, task_id, &format!("fetch.already_owned: {note}"))?;
            return Err("fetch.already_owned".into());
        }
    }

    if dest.exists() {
        match hash::full_hash(&dest) {
            Ok((_, got, ..)) if got.eq_ignore_ascii_case(&sha) => {
                finish_success(conn, task_id, wanted_id, &root, &rel)?;
                return Ok(FetchResult {
                    task_id,
                    wanted_id,
                    state: "done".into(),
                    dest: dest.to_string_lossy().into_owned(),
                });
            }
            Ok(_) => {
                task_error(conn, task_id, "fetch.dest_mismatch")?;
                return Err("fetch.dest_mismatch".into());
            }
            Err(e) => {
                task_error(conn, task_id, &format!("scan.io: {e}"))?;
                return Err(format!("scan.io: {e}"));
            }
        }
    }

    let stage_dir = Path::new(&root.path).join(".janus-partial");
    fs::create_dir_all(&stage_dir).map_err(|e| format!("scan.io: {e}"))?;
    let stage = stage_dir.join(format!("{wanted_id}.part"));
    conn.execute(
        "UPDATE fetch_tasks SET state='running', dest_rel_path=?1 WHERE id=?2",
        params![rel, task_id],
    )
    .map_err(store::to_err)?;

    if let Err(e) = source.download(&wanted, &stage) {
        let _ = fs::remove_file(&stage);
        task_error(conn, task_id, &e)?;
        return Err(e);
    }

    let (_, got, ..) = hash::full_hash(&stage).map_err(|e| {
        let _ = fs::remove_file(&stage);
        format!("scan.io: {e}")
    })?;
    if !got.eq_ignore_ascii_case(&sha) {
        let _ = fs::remove_file(&stage);
        task_error(conn, task_id, "fetch.dest_mismatch")?;
        return Err("fetch.dest_mismatch".into());
    }

    fsync_path(&stage)?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("scan.io: {e}"))?;
        if parent.is_symlink() {
            let _ = fs::remove_file(&stage);
            task_error(conn, task_id, "fetch.path_invalid")?;
            return Err("fetch.path_invalid".into());
        }
    }
    fs::rename(&stage, &dest).map_err(|e| {
        let _ = fs::remove_file(&stage);
        format!("scan.io: {e}")
    })?;
    fsync_parent(&dest)?;

    finish_success(conn, task_id, wanted_id, &root, &rel)?;
    Ok(FetchResult {
        task_id,
        wanted_id,
        state: "done".into(),
        dest: dest.to_string_lossy().into_owned(),
    })
}

fn finish_success(conn: &Connection, task_id: i64, wanted_id: i64, root: &store::RootRow, rel: &str) -> Result<(), String> {
    let _ = scan::scan_root(conn, root.id, &ScanOptions { quick: false });
    conn.execute(
        "UPDATE fetch_tasks SET state='done', error=NULL, dest_rel_path=?1 WHERE id=?2",
        params![rel, task_id],
    )
    .map_err(store::to_err)?;
    conn.execute("UPDATE wanted_items SET status='fetched' WHERE id=?1", [wanted_id])
        .map_err(store::to_err)?;
    if let Ok(wanted) = radar::wanted_by_id(conn, wanted_id) {
        if let Some(fid) = wanted.family_id {
            store::provenance_put(conn, "family", fid, "downloaded_from", "hf", Some(&wanted.repo), Some(&wanted.revision));
        }
    }
    Ok(())
}

fn task_upsert(conn: &Connection, wanted_id: i64, dest_root_id: i64, dest_rel: &str, state: &str) -> Result<i64, String> {
    if let Ok(id) = conn.query_row(
        "SELECT id FROM fetch_tasks WHERE wanted_id=?1 AND state IN ('queued','running','paused')",
        [wanted_id],
        |r| r.get::<_, i64>(0),
    ) {
        conn.execute(
            "UPDATE fetch_tasks SET dest_root_id=?1, dest_rel_path=?2, state=?3, error=NULL WHERE id=?4",
            params![dest_root_id, dest_rel, state, id],
        )
        .map_err(store::to_err)?;
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO fetch_tasks (wanted_id, dest_root_id, dest_rel_path, bytes_done, bytes_total, state)
         VALUES (?1,?2,?3,0,0,?4)",
        params![wanted_id, dest_root_id, dest_rel, state],
    )
    .map_err(store::to_err)?;
    Ok(conn.last_insert_rowid())
}

fn task_error(conn: &Connection, id: i64, error: &str) -> Result<(), String> {
    conn.execute("UPDATE fetch_tasks SET state='error', error=?1 WHERE id=?2", params![error, id])
        .map_err(store::to_err)?;
    Ok(())
}

fn fsync_path(path: &Path) -> Result<(), String> {
    let f = OpenOptions::new().read(true).write(true).open(path).map_err(|e| format!("scan.io: {e}"))?;
    f.sync_all().map_err(|e| format!("scan.io: {e}"))
}

fn fsync_parent(path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

pub fn task_get(conn: &Connection, id: i64) -> Result<store::JobRow, String> {
    conn.query_row(
        "SELECT id, 'fetch', COALESCE(state,''), NULL, NULL, NULL, error FROM fetch_tasks WHERE id=?1",
        [id],
        |r| {
            Ok(store::JobRow {
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

pub fn task_by_wanted(conn: &Connection, wanted_id: i64) -> Option<i64> {
    conn.query_row("SELECT id FROM fetch_tasks WHERE wanted_id=?1 ORDER BY id DESC LIMIT 1", [wanted_id], |r| r.get(0))
        .ok()
}

pub fn task_list(conn: &Connection) -> Result<Vec<serde_json::Value>, String> {
    let mut stmt = conn
        .prepare("SELECT id, wanted_id, dest_rel_path, COALESCE(state,''), error FROM fetch_tasks ORDER BY id DESC")
        .map_err(store::to_err)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "wanted_id": r.get::<_, i64>(1)?,
                "dest_rel_path": r.get::<_, String>(2)?,
                "state": r.get::<_, String>(3)?,
                "error": r.get::<_, Option<String>>(4)?,
            }))
        })
        .map_err(store::to_err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(store::to_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use sha2::{Digest, Sha256};

    fn mem() -> Connection {
        let c = db::open(None).unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    fn sha(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn seed_fetch_root(c: &Connection) -> (PathBuf, i64) {
        let dir = std::env::temp_dir().join(format!("janus-fetch-root-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        let id = store::root_add_opts(c, "inbound", dir.to_str().unwrap(), "fetch", true).unwrap();
        (dir, id)
    }

    fn seed_wanted(c: &Connection, filename: &str, digest: Option<&str>) -> i64 {
        c.execute(
            "INSERT INTO wanted_items (remote_key, provider, repo, revision, filename, size, sha256, status)
             VALUES (?1,'hf','acme/Qwen','main',?2,4,?3,'open')",
            params![format!("hf|acme/Qwen|main|{filename}"), filename, digest],
        )
        .unwrap();
        c.last_insert_rowid()
    }

    #[test]
    fn dest_rejects_traversal() {
        let root = PathBuf::from("C:/models/inbound");
        assert_eq!(validate_dest_rel(&root, "../escape.gguf").unwrap_err(), "fetch.path_invalid");
        assert_eq!(validate_dest_rel(&root, "/abs/x.gguf").unwrap_err(), "fetch.path_invalid");
        assert_eq!(validate_dest_rel(&root, "C:\\Windows\\x.gguf").unwrap_err(), "fetch.path_invalid");
        assert_eq!(validate_dest_rel(&root, "\\\\server\\share\\x.gguf").unwrap_err(), "fetch.path_invalid");
        let ok = validate_dest_rel(&root, "Qwen-Q4_K_M.gguf").unwrap();
        assert!(ok.starts_with(&root));
    }

    #[test]
    fn null_digest_fails_closed_no_install() {
        let c = mem();
        let (dir, _) = seed_fetch_root(&c);
        let id = seed_wanted(&c, "a.gguf", None);
        let err = fetch_wanted(&c, id, None, false, &MemoryBytes(b"data".to_vec())).unwrap_err();
        assert_eq!(err, "wanted.no_sha256");
        assert!(!dir.join("a.gguf").exists());
        let state: String = c.query_row("SELECT state FROM fetch_tasks WHERE wanted_id=?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(state, "error");
        let status: String = c.query_row("SELECT status FROM wanted_items WHERE id=?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(status, "open");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn already_owned_refuses_without_force() {
        let c = mem();
        let (fetch_dir, _) = seed_fetch_root(&c);
        let drawer = std::env::temp_dir().join(format!("janus-fetch-drawer-{}", std::process::id()));
        std::fs::create_dir_all(&drawer).unwrap();
        let root = store::root_add_opts(&c, "drawer", drawer.to_str().unwrap(), "removable", true).unwrap();
        c.execute("UPDATE storage_roots SET present=0 WHERE id=?1", [root]).unwrap();
        let bytes = b"owned";
        let digest = sha(bytes);
        let blob = store::blob_upsert(&c, "blake-owned", Some(&digest), bytes.len() as i64, None).unwrap();
        store::file_upsert(&c, root, "have.gguf", bytes.len() as i64, 0, 0, 0, 9, true, None, Some(blob), "full", "ok", None).unwrap();
        let id = seed_wanted(&c, "b.gguf", Some(&digest));
        let err = fetch_wanted(&c, id, None, false, &MemoryBytes(bytes.to_vec())).unwrap_err();
        assert_eq!(err, "fetch.already_owned");
        assert!(!fetch_dir.join("b.gguf").exists());
        let ok = fetch_wanted(&c, id, None, true, &MemoryBytes(bytes.to_vec())).unwrap();
        assert_eq!(ok.state, "done");
        assert!(fetch_dir.join("b.gguf").exists());
        let _ = std::fs::remove_dir_all(&fetch_dir);
        let _ = std::fs::remove_dir_all(&drawer);
    }

    #[test]
    fn existing_dest_matching_sha_is_fetched_not_overwritten() {
        let c = mem();
        let (dir, _) = seed_fetch_root(&c);
        let bytes = b"same";
        let digest = sha(bytes);
        let dest = dir.join("keep.gguf");
        std::fs::write(&dest, bytes).unwrap();
        let before = std::fs::metadata(&dest).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let id = seed_wanted(&c, "keep.gguf", Some(&digest));
        let res = fetch_wanted(&c, id, Some("keep.gguf"), false, &MemoryBytes(b"DIFFERENT".to_vec())).unwrap();
        assert_eq!(res.state, "done");
        let after = std::fs::metadata(&dest).unwrap().modified().unwrap();
        assert_eq!(before, after);
        assert_eq!(std::fs::read(&dest).unwrap(), bytes);
        let status: String = c.query_row("SELECT status FROM wanted_items WHERE id=?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(status, "fetched");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stages_under_janus_partial_then_renames() {
        let c = mem();
        let (dir, _) = seed_fetch_root(&c);
        let bytes = b"gguf";
        let digest = sha(bytes);
        let id = seed_wanted(&c, "new.gguf", Some(&digest));
        let res = fetch_wanted(&c, id, None, false, &MemoryBytes(bytes.to_vec())).unwrap();
        assert_eq!(res.state, "done");
        assert!(dir.join("new.gguf").exists());
        assert!(!dir.join(".janus-partial").join(format!("{id}.part")).exists());
        let status: String = c.query_row("SELECT status FROM wanted_items WHERE id=?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(status, "fetched");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
