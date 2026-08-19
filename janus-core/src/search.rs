use rusqlite::{Connection, params};

#[derive(Debug, Clone)]
pub struct Hit {
    pub kind: &'static str,
    pub name: String,
    pub key: Option<String>,
    pub path: Option<String>,
    pub present: bool,
}

pub fn search(conn: &Connection, query: &str) -> Result<Vec<Hit>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let like = format!("%{}%", q.to_lowercase());
    let mut out = Vec::new();

    let mut fams = conn
        .prepare(
            "SELECT name, family_key FROM model_families
             WHERE lower(COALESCE(name,'')) LIKE ?1 OR lower(family_key) LIKE ?1
             ORDER BY COALESCE(name, family_key)",
        )
        .map_err(|e| format!("search:{e}"))?;
    let rows = fams
        .query_map([&like], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| format!("search:{e}"))?;
    for row in rows.flatten() {
        let (name, key) = row;
        out.push(Hit {
            kind: "family",
            name: name.unwrap_or_else(|| key.split('|').next().unwrap_or("unknown").to_string()),
            key: Some(key),
            path: None,
            present: true,
        });
    }

    let mut files = conn
        .prepare(
            "SELECT f.rel_path, r.name, COALESCE(r.present,0), b.blake3, b.sha256
             FROM files f
             JOIN storage_roots r ON r.id=f.root_id
             LEFT JOIN blobs b ON b.id=f.blob_id
             WHERE lower(f.rel_path) LIKE ?1
                OR lower(COALESCE(b.blake3,'')) LIKE ?1
                OR lower(COALESCE(b.sha256,'')) LIKE ?1
             ORDER BY f.rel_path",
        )
        .map_err(|e| format!("search:{e}"))?;
    let frows = files
        .query_map(params![like], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(|e| format!("search:{e}"))?;
    for row in frows.flatten() {
        let (rel, root, present, blake3, _sha256) = row;
        out.push(Hit {
            kind: "file",
            name: rel.clone(),
            key: blake3,
            path: Some(format!("{root}/{rel}")),
            present: present == 1,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, store};

    fn mem() -> Connection {
        let c = db::open(None).unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    #[test]
    fn search_finds_family_by_name() {
        let c = mem();
        store::family_insert(&c, "qwen3-coder|qwen3|t30|a30", Some("Qwen3-Coder"), Some("qwen3"), Some(30.0), None, None, "llm")
            .unwrap();
        let hits = search(&c, "qwen").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "family");
        assert_eq!(hits[0].name, "Qwen3-Coder");
    }

    #[test]
    fn search_empty_query_is_empty() {
        let c = mem();
        store::family_insert(&c, "qwen3-coder|qwen3|t30|a30", Some("Qwen3-Coder"), None, None, None, None, "llm").unwrap();
        assert!(search(&c, "   ").unwrap().is_empty());
    }
}
