use rusqlite::Connection;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DupCopy {
    pub root_id: i64,
    pub root_name: String,
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

pub fn plan(conn: &Connection) -> Vec<DupGroup> {
    let mut groups: HashMap<String, DupGroup> = HashMap::new();
    let mut stmt = match conn.prepare(
        "SELECT f.root_id, r.name, f.rel_path, f.dev, f.ino, f.hash_state, b.blake3, b.size
         FROM files f
         JOIN blobs b ON b.id = f.blob_id
         JOIN storage_roots r ON r.id = f.root_id
         WHERE f.hash_state = 'full' AND f.blob_id IS NOT NULL",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, i64>(7)?,
            ))
        })
        .ok();
    let Some(rows) = rows else {
        return Vec::new();
    };
    for row in rows.flatten() {
        let (root_id, root_name, rel_path, dev, ino, hash_state, blake3, size) = row;
        let g = groups.entry(blake3.clone()).or_insert_with(|| DupGroup {
            blake3: blake3.clone(),
            size,
            ..Default::default()
        });
        g.copies.push(DupCopy {
            root_id,
            root_name,
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
            let mut allocs: Vec<(i64, i64)> = g
                .copies
                .iter()
                .map(|c| (c.dev, c.ino))
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