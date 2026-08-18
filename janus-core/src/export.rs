use rusqlite::Connection;
use serde_json::{Value, json};

fn rows(conn: &Connection, sql: &str) -> Result<Vec<serde_json::Map<String, Value>>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| format!("export:{e}"))?;
    let cols: Vec<String> = stmt
        .column_names()
        .into_iter()
        .map(|c| c.to_string())
        .collect();
    let n = cols.len();
    let mut out = Vec::new();
    let mut iter = stmt.query([]).map_err(|e| format!("export:{e}"))?;
    while let Some(r) = iter.next().map_err(|e| format!("export:{e}"))? {
        let mut m = serde_json::Map::new();
        for i in 0..n {
            let v = match r.get_ref(i).map_err(|e| format!("export:{e}"))? {
                rusqlite::types::ValueRef::Integer(iv) => Value::from(iv),
                rusqlite::types::ValueRef::Real(f) => Value::from(f),
                rusqlite::types::ValueRef::Text(t) => Value::from(String::from_utf8_lossy(t).into_owned()),
                rusqlite::types::ValueRef::Blob(b) => Value::from(b.to_vec()),
                rusqlite::types::ValueRef::Null => Value::Null,
            };
            m.insert(cols[i].clone(), v);
        }
        out.push(m);
    }
    Ok(out)
}

pub fn export(conn: &Connection) -> Result<Value, String> {
    Ok(json!({
        "format": "janus.export",
        "format_version": 1,
        "schema_version": crate::SCHEMA_VERSION,
        "family_key_algo": crate::FAMILY_KEY_ALGO,
        "exported_at": exported_at(),
        "roots": Value::Array(rows(conn,
            "SELECT id,name,path,kind,mode,present,last_scan_at FROM storage_roots ORDER BY id")?
            .into_iter().map(Value::Object).collect()),
        "families": Value::Array(rows(conn,
            "SELECT id,family_key,name,arch,params_total,params_active,context_len,kind FROM model_families ORDER BY family_key")?
            .into_iter().map(Value::Object).collect()),
        "blobs": Value::Array(rows(conn,
            "SELECT b.id,b.blake3,b.sha256,b.size,
                    (SELECT COUNT(*) FROM files f WHERE f.blob_id=b.id) AS refcount
             FROM blobs b ORDER BY b.id")?
            .into_iter().map(Value::Object).collect()),
        "family_aliases": Value::Array(rows(conn,
            "SELECT f.family_key, a.alias, a.source
             FROM family_aliases a JOIN model_families f ON f.id=a.family_id
             ORDER BY f.family_key")?
            .into_iter().map(Value::Object).collect()),
        "declined_merges": Value::Array(rows(conn,
            "SELECT family_a_key, family_b_key, algo_version, declined_at FROM declined_merges ORDER BY family_a_key, family_b_key")?
            .into_iter().map(Value::Object).collect()),
        "files": Value::Array(rows(conn,
            "SELECT f.id, r.name AS root_name, f.rel_path, f.size, f.hash_state, f.parse_state, b.blake3
             FROM files f JOIN storage_roots r ON r.id=f.root_id LEFT JOIN blobs b ON b.id=f.blob_id
             ORDER BY f.id")?
            .into_iter().map(Value::Object).collect()),
    }))
}

fn exported_at() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[derive(Debug, Clone, Default)]
pub struct ImportReport {
    pub families: u64,
    pub aliases: u64,
    pub declined: u64,
}

pub fn import(conn: &Connection, v: &Value) -> Result<ImportReport, String> {
    if v.get("format").and_then(|x| x.as_str()) != Some("janus.export")
        || v.get("format_version").and_then(|x| x.as_u64()) != Some(1)
        || v.get("family_aliases").is_none()
        || v.get("declined_merges").is_none()
    {
        return Err("export.incomplete".to_string());
    }
    match v.get("family_key_algo").and_then(|x| x.as_str()) {
        Some(a) if a == crate::FAMILY_KEY_ALGO => {}
        _ => return Err("export.algo_mismatch".to_string()),
    }

    let mut report = ImportReport::default();
    if let Some(fams) = v.get("families").and_then(|x| x.as_array()) {
        for fam in fams {
            let key = match fam.get("family_key").and_then(|x| x.as_str()) {
                Some(k) if !k.is_empty() => k,
                _ => continue,
            };
            if crate::store::family_find(conn, key).is_some() {
                continue;
            }
            crate::store::family_insert(
                conn,
                key,
                fam.get("name").and_then(|x| x.as_str()),
                fam.get("arch").and_then(|x| x.as_str()),
                fam.get("params_total").and_then(|x| x.as_f64()),
                fam.get("params_active").and_then(|x| x.as_f64()),
                fam.get("context_len").and_then(|x| x.as_i64()),
                fam.get("kind").and_then(|x| x.as_str()).unwrap_or("unknown"),
            )?;
            report.families += 1;
        }
    }

    if let Some(aliases) = v.get("family_aliases").and_then(|x| x.as_array()) {
        for a in aliases {
            let alias = a.get("alias").and_then(|x| x.as_str()).unwrap_or("");
            let family_key = a.get("family_key").and_then(|x| x.as_str()).unwrap_or("");
            if alias.is_empty() || family_key.is_empty() {
                continue;
            }
            let Some(fid) = crate::store::family_find(conn, family_key) else {
                continue;
            };
            conn.execute(
                "INSERT OR IGNORE INTO family_aliases (family_id, alias, source) VALUES (?1, ?2, ?3)",
                rusqlite::params![fid, alias, a.get("source").and_then(|x| x.as_str()).unwrap_or("import")],
            )
            .map_err(|e| format!("import:{e}"))?;
            report.aliases += 1;
        }
    }

    if let Some(declined) = v.get("declined_merges").and_then(|x| x.as_array()) {
        for d in declined {
            let a = d.get("family_a_key").and_then(|x| x.as_str()).unwrap_or("");
            let b = d.get("family_b_key").and_then(|x| x.as_str()).unwrap_or("");
            let algo = d.get("algo_version").and_then(|x| x.as_str()).unwrap_or(crate::FAMILY_KEY_ALGO);
            if a.is_empty() || b.is_empty() {
                continue;
            }
            crate::store::declined_merge(conn, a, b, algo)?;
            report.declined += 1;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, store};

    fn mem() -> rusqlite::Connection {
        let c = db::open(None).unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    #[test]
    fn import_rejects_algo_mismatch() {
        let c = mem();
        let v = json!({
            "format": "janus.export",
            "format_version": 1,
            "family_key_algo": "999",
            "family_aliases": [],
            "declined_merges": []
        });
        assert_eq!(import(&c, &v).unwrap_err(), "export.algo_mismatch");
    }

    #[test]
    fn import_rejects_missing_decisions() {
        let c = mem();
        let v = json!({
            "format": "janus.export",
            "format_version": 1,
            "family_key_algo": "1",
            "declined_merges": []
        });
        assert_eq!(import(&c, &v).unwrap_err(), "export.incomplete");
    }

    #[test]
    fn export_import_roundtrip_aliases_and_declines() {
        let src = mem();
        store::family_insert(&src, "bar|llama|t8|a8", Some("Bar"), Some("llama"), Some(8.0), Some(8.0), None, "llm").unwrap();
        store::family_insert(&src, "foo|llama|t8|a8", Some("Foo"), Some("llama"), Some(8.0), Some(8.0), None, "llm").unwrap();
        store::merge_families(&src, "Foo", "Bar").unwrap();
        store::declined_merge(&src, "aaa|x|t1|a1", "bbb|x|t1|a1", crate::FAMILY_KEY_ALGO).unwrap();
        let manifest = export(&src).unwrap();
        assert_eq!(manifest["format"], "janus.export");
        assert!(manifest.get("family_aliases").is_some());

        let dest = mem();
        let report = import(&dest, &manifest).unwrap();
        assert!(report.families >= 1);
        assert!(report.aliases >= 1);
        assert!(report.declined >= 1);
        assert!(store::family_resolve(&dest, "foo|llama|t8|a8").is_some());
        assert!(store::is_declined(&dest, "aaa|x|t1|a1", "bbb|x|t1|a1", crate::FAMILY_KEY_ALGO));
    }
}