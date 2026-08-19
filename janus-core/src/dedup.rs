use rusqlite::Connection;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DupCopy {
    pub root_id: i64,
    pub root_name: String,
    pub mount_id: Option<String>,
    pub rel_path: String,
    pub dev: i64,
    pub ino: i64,
    pub hash_state: String,
}

#[derive(Debug, Clone, Default)]
pub struct DupGroup {
    pub blake3: String,
    pub size: i64,
    pub copies: Vec<DupCopy>,
    pub allocations: i64,
    pub reclaimable_files: i64,
    pub reclaimable_bytes: i64,
}

fn alloc_key(c: &DupCopy) -> String {
    c.mount_id
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("root:{}", c.root_id))
}

pub fn plan(conn: &Connection) -> Vec<DupGroup> {
    let mut groups: HashMap<String, DupGroup> = HashMap::new();
    let mut stmt = match conn.prepare(
        "SELECT f.root_id, r.name, r.mount_id, f.rel_path, f.dev, f.ino, f.hash_state, b.blake3, b.size
         FROM files f
         JOIN blobs b ON b.id = f.blob_id
         JOIN storage_roots r ON r.id = f.root_id
         WHERE f.hash_state = 'full' AND f.blob_id IS NOT NULL AND f.state = 'present'",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, i64>(8)?,
            ))
        })
        .ok();
    let Some(rows) = rows else {
        return Vec::new();
    };
    for row in rows.flatten() {
        let (root_id, root_name, mount_id, rel_path, dev, ino, hash_state, blake3, size) = row;
        let g = groups.entry(blake3.clone()).or_insert_with(|| DupGroup {
            blake3: blake3.clone(),
            size,
            ..Default::default()
        });
        g.copies.push(DupCopy {
            root_id,
            root_name,
            mount_id,
            rel_path,
            dev,
            ino,
            hash_state,
        });
    }
    let mut out: Vec<DupGroup> = groups
        .into_values()
        .filter(|g| g.copies.len() > 1)
        .map(|mut g| {
            let mut allocs: Vec<(String, i64, i64)> = g
                .copies
                .iter()
                .map(|c| (alloc_key(c), c.dev, c.ino))
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            allocs.sort();
            g.allocations = allocs.len() as i64;
            g.reclaimable_files = (g.allocations - 1).max(0);
            g.reclaimable_bytes = g.reclaimable_files * g.size;
            g
        })
        .collect();
    out.sort_by(|a, b| a.blake3.cmp(&b.blake3));
    out
}

pub fn have_bytes(conn: &Connection, file_id: i64) -> bool {
    matches!(
        conn.query_row(
            "SELECT (hash_state = 'full' AND blob_id IS NOT NULL) FROM files WHERE id=?1",
            [file_id],
            |r| r.get::<_, bool>(0)
        ),
        Ok(true)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use rusqlite::params;

    fn mem() -> Connection {
        let c = db::open(None).unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    fn add_copy(c: &Connection, name: &str, path: &str, mount: Option<&str>, dev: i64, ino: i64, rel: &str) {
        c.execute(
            "INSERT INTO storage_roots (name, path, kind, mode, writable, mount_id, present)
             VALUES (?1, ?2, 'internal', 'catalogue', 0, ?3, 1)",
            params![name, path, mount],
        )
        .unwrap();
        let root_id = c.last_insert_rowid();
        let blob = crate::store::blob_upsert(c, "abc", None, 1000, None).unwrap();
        c.execute(
            "INSERT INTO files (root_id, rel_path, size, mtime, ctime, dev, ino, is_symlink, blob_id, hash_state, parse_state, state)
             VALUES (?1, ?2, 1000, 0, 0, ?3, ?4, 0, ?5, 'full', 'ok', 'present')",
            params![root_id, rel, dev, ino, blob],
        )
        .unwrap();
    }

    #[test]
    fn same_dev_ino_on_different_mounts_are_two_allocations() {
        let c = mem();
        add_copy(&c, "disk-a", "/a", Some("VOL-A"), 8, 100, "m.gguf");
        add_copy(&c, "disk-b", "/b", Some("VOL-B"), 8, 100, "m.gguf");
        let plan = plan(&c);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].allocations, 2, "same (dev,ino) on two volumes is two allocations");
        assert_eq!(plan[0].reclaimable_bytes, 1000);
    }

    #[test]
    fn hardlink_on_one_mount_is_one_allocation() {
        let c = mem();
        add_copy(&c, "models", "/m", Some("VOL-A"), 8, 100, "a.gguf");
        let root_id: i64 = c.query_row("SELECT id FROM storage_roots", [], |r| r.get(0)).unwrap();
        let blob: i64 = c.query_row("SELECT id FROM blobs", [], |r| r.get(0)).unwrap();
        c.execute(
            "INSERT INTO files (root_id, rel_path, size, mtime, ctime, dev, ino, is_symlink, blob_id, hash_state, parse_state, state)
             VALUES (?1, 'copy.gguf', 1000, 0, 0, 8, 100, 0, ?2, 'full', 'ok', 'present')",
            params![root_id, blob],
        )
        .unwrap();
        let plan = plan(&c);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].allocations, 1);
        assert_eq!(plan[0].reclaimable_bytes, 0);
    }
}