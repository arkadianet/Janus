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
        "schema_version": crate::SCHEMA_VERSION,
        "family_key_algo": crate::FAMILY_KEY_ALGO,
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
        "aliases": Value::Array(rows(conn,
            "SELECT f.family_key AS from_key, a.alias AS to_key, a.source
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