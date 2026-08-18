use crate::{db, dedup, doctor, export, scan, store};
use rusqlite::{Connection, params};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct CaseReport {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub detail: String,
}

#[derive(Deserialize, Default)]
struct Case {
    id: String,
    title: String,
    #[serde(default)]
    #[allow(dead_code)]
    header_required: bool,
    #[serde(default)]
    scan: ScanCfg,
    #[serde(default)]
    files: Vec<FiFile>,
    #[serde(default)]
    expect: Expect,
    #[serde(default)]
    given: Given,
}

#[derive(Deserialize, Default)]
struct ScanCfg {
    #[serde(default)]
    quick: bool,
}

#[derive(Deserialize)]
struct FiFile {
    name: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    quant_from: String,
    #[serde(default)]
    expect_quant: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    known_basename: Option<String>,
    #[serde(default)]
    known_params_total: Option<f64>,
    #[serde(default)]
    known_params_active: Option<f64>,
    #[serde(default)]
    publisher: Option<String>,
    #[serde(default)]
    same_blob: Option<String>,
    #[serde(default)]
    metadata_empty: bool,
}

#[derive(Deserialize, Default)]
struct Expect {
    #[serde(default)]
    family_count: Option<i64>,
    #[serde(default)]
    blob_count: Option<i64>,
    #[serde(default)]
    file_count: Option<i64>,
    #[serde(default)]
    reclaimable_inodes: i64,
    #[serde(default)]
    variant_quants: Vec<String>,
    #[serde(default)]
    publishers: Vec<String>,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    silent_merge: bool,
    #[serde(default)]
    merge_suggestion: bool,
    #[serde(default)]
    hash_state: Option<String>,
    #[serde(default)]
    have_bytes: bool,
    #[serde(default)]
    family_aliases_rows: i64,
    #[serde(default)]
    declined_merges_rows: i64,
    #[serde(default)]
    resweep_nags: bool,
    #[serde(default)]
    export_includes: Vec<String>,
    #[serde(default)]
    unknown: bool,
    #[serde(default)]
    searchable: bool,
    #[serde(default)]
    no_invented_family_name: bool,
}

#[derive(Deserialize, Default)]
struct Given {
    #[serde(default)]
    declined: Option<Declined>,
}

#[derive(Deserialize)]
struct Declined {
    family_a_key: String,
    family_b_key: String,
    algo_version: String,
}

pub fn run_all(fixtures: &Path) -> Vec<CaseReport> {
    let cases_dir = fixtures.join("cases");
    let mut tomls = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&cases_dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x == "toml").unwrap_or(false) {
                tomls.push(p);
            }
        }
    }
    tomls.sort();
    tomls.into_iter()
        .filter_map(|p| {
            let src = std::fs::read_to_string(&p).ok()?;
            let case: Case = toml::from_str(&src).ok()?;
            Some(run_one(&case, fixtures))
        })
        .collect()
}

fn run_one(case: &Case, fixtures: &Path) -> CaseReport {
    let conn = match db::open(None).and_then(|c| db::init_schema(&c).map(|_| c)) {
        Ok(c) => c,
        Err(e) => return report(case, Status::Fail, format!("db:{e}")),
    };

    let root_dir = match materialize_root(case, fixtures) {
        Ok(Some(d)) => d,
        Ok(None) => return report(case, Status::Skipped, "missing fixture files".to_string()),
        Err(e) => return report(case, Status::Fail, format!("materialize:{e}")),
    };

    if let Some(d) = &case.given.declined {
        if let Err(e) = store::declined_merge(&conn, &d.family_a_key, &d.family_b_key, &d.algo_version) {
            return report(case, Status::Fail, format!("declined:{e}"));
        }
    }

    let root_id = match store::root_add(&conn, &case.id, root_dir.to_string_lossy().as_ref(), "internal") {
        Ok(id) => id,
        Err(e) => return report(case, Status::Fail, format!("root:{e}")),
    };

    let opts = scan::ScanOptions { quick: case.scan.quick };
    if let Err(e) = scan::scan_root(&conn, root_id, &opts) {
        return report(case, Status::Fail, format!("scan:{e}"));
    }

    for f in &case.files {
        if let Some(pubname) = &f.publisher {
            let _ = conn.execute(
                "UPDATE model_variants SET publisher=?1 WHERE id=(
                    SELECT fr.variant_id FROM file_roles fr
                    JOIN files f ON f.id=fr.file_id
                    WHERE f.root_id=?2 AND f.rel_path=?3 LIMIT 1)",
                rusqlite::params![pubname, root_id, f.name],
            );
        }
    }

    match assert_expectations(case, &conn, root_id) {
        Ok(()) => report(case, Status::Pass, "ok".to_string()),
        Err(detail) => report(case, Status::Fail, detail),
    }
}

fn report(case: &Case, status: Status, detail: String) -> CaseReport {
    CaseReport {
        id: case.id.clone(),
        title: case.title.clone(),
        status,
        detail,
    }
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0)
}

fn assert_expectations(case: &Case, conn: &Connection, root_id: i64) -> Result<(), String> {
    let mut fail: Vec<String> = Vec::new();

    for f in &case.files {
        if let Some(role) = &f.role {
            let got: Option<String> = conn
                .query_row(
                    "SELECT fr.role FROM file_roles fr JOIN files f ON f.id=fr.file_id WHERE f.root_id=?1 AND f.rel_path=?2",
                    params![root_id, f.name],
                    |r| r.get(0),
                )
                .ok();
            if got.as_deref() != Some(role.as_str()) {
                fail.push(format!("file {} role want {role} got {:?}", f.name, got));
            }
        }
        if let Some(q) = &f.expect_quant {
            let got: Option<String> = conn
                .query_row(
                    "SELECT v.quant FROM file_roles fr JOIN model_variants v ON v.id=fr.variant_id
                     JOIN files f ON f.id=fr.file_id WHERE f.root_id=?1 AND f.rel_path=?2",
                    params![root_id, f.name],
                    |r| r.get(0),
                )
                .ok();
            if got.as_deref() != Some(q.as_str()) {
                fail.push(format!("file {} quant want {q} got {:?}", f.name, got));
            }
        }
    }

    if case.expect.searchable && case.expect.unknown {
        let has: i64 = count(
            conn,
            "SELECT COUNT(*) FROM files f WHERE f.blob_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM file_roles fr WHERE fr.file_id=f.id)",
        );
        if has == 0 {
            fail.push("searchable want blob on unnamed file".to_string());
        }
    }

    if let Some(want) = case.expect.family_count {
        let got = count(conn, "SELECT COUNT(*) FROM model_families");
        if got != want {
            fail.push(format!("family_count want {want} got {got}"));
        }
    }
    if let Some(want) = case.expect.blob_count {
        let got = count(conn, "SELECT COUNT(*) FROM blobs");
        if got != want {
            fail.push(format!("blob_count want {want} got {got}"));
        }
    }
    if let Some(want) = case.expect.file_count {
        let got = count(conn, "SELECT COUNT(*) FROM files");
        if got != want {
            fail.push(format!("file_count want {want} got {got}"));
        }
    }
    if case.expect.reclaimable_inodes != 0 {
        let got: i64 = dedup::plan(conn).iter().map(|g| g.reclaimable_files).sum();
        if got != case.expect.reclaimable_inodes {
            fail.push(format!("reclaimable_inodes want {} got {got}", case.expect.reclaimable_inodes));
        }
    }
    let mut sorted = |mut want: Vec<String>| {
        want.sort();
        want
    };
    if !case.expect.variant_quants.is_empty() {
        let mut got: Vec<String> = conn
            .prepare("SELECT quant FROM model_variants ORDER BY quant")
            .and_then(|mut s| s.query_map([], |r| r.get(0)).map(|it| it.collect::<Result<Vec<String>, _>>()))
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default();
        got.sort();
        if got != sorted(case.expect.variant_quants.clone()) {
            fail.push(format!("variant_quants want {:?} got {:?}", case.expect.variant_quants, got));
        }
    }
    if !case.expect.publishers.is_empty() {
        let mut got: Vec<String> = conn
            .prepare("SELECT DISTINCT publisher FROM model_variants ORDER BY publisher")
            .and_then(|mut s| s.query_map([], |r| r.get(0)).map(|it| it.collect::<Result<Vec<String>, _>>()))
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default();
        got.sort();
        if got != sorted(case.expect.publishers.clone()) {
            fail.push(format!("publishers want {:?} got {:?}", case.expect.publishers, got));
        }
    }
    if !case.expect.roles.is_empty() {
        let mut got: Vec<String> = conn
            .prepare("SELECT DISTINCT role FROM file_roles ORDER BY role")
            .and_then(|mut s| s.query_map([], |r| r.get(0)).map(|it| it.collect::<Result<Vec<String>, _>>()))
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default();
        got.sort();
        if got != sorted(case.expect.roles.clone()) {
            fail.push(format!("roles want {:?} got {:?}", case.expect.roles, got));
        }
    }
    if let Some(hs) = &case.expect.hash_state {
        let got: String = conn
            .query_row("SELECT hash_state FROM files WHERE root_id=?1 LIMIT 1", [root_id], |r| r.get(0))
            .unwrap_or_default();
        if &got != hs {
            fail.push(format!("hash_state want {hs} got {got}"));
        }
    }
    if case.expect.have_bytes {
        let has: i64 = count(conn, "SELECT COUNT(*) FROM files WHERE hash_state='full' AND blob_id IS NOT NULL");
        if has == 0 {
            fail.push("have_bytes want true got false".to_string());
        }
    }
    if case.expect.family_aliases_rows != 0 {
        let got = count(conn, "SELECT COUNT(*) FROM family_aliases");
        if got != case.expect.family_aliases_rows {
            fail.push(format!("family_aliases_rows want {} got {got}", case.expect.family_aliases_rows));
        }
    }
    if case.expect.declined_merges_rows != 0 {
        let got = count(conn, "SELECT COUNT(*) FROM declined_merges");
        if got != case.expect.declined_merges_rows {
            fail.push(format!("declined_merges_rows want {} got {got}", case.expect.declined_merges_rows));
        }
    }
    if !case.expect.export_includes.is_empty() {
        match export::export(conn) {
            Ok(v) => {
                for k in &case.expect.export_includes {
                    if v.get(k).is_none() {
                        fail.push(format!("export missing key {k}"));
                    }
                }
            }
            Err(e) => fail.push(format!("export err:{e}")),
        }
    }
    if case.expect.merge_suggestion {
        if doctor::sweep(conn).is_empty() {
            fail.push("merge_suggestion want true got none".to_string());
        }
    }
    if case.expect.resweep_nags {
        let nags = doctor::sweep(conn);
        if let Some(d) = &case.given.declined {
            let a = &d.family_a_key;
            let b = &d.family_b_key;
            if nags.iter().any(|s| (s.a_key == *a && s.b_key == *b) || (s.a_key == *b && s.b_key == *a)) {
                fail.push("resweep_nags want no nag for declined pair".to_string());
            }
        }
        if !nags.is_empty() {
            fail.push("resweep_nags want empty sweep".to_string());
        }
    }
    if case.expect.unknown {
        let fams = count(conn, "SELECT COUNT(*) FROM model_families");
        if fams > 0 {
            fail.push("unknown want no family".to_string());
        }
    }
    if case.expect.no_invented_family_name {
        let fams = count(conn, "SELECT COUNT(*) FROM model_families");
        if fams != 0 {
            fail.push(format!("no_invented_family_name want 0 families got {fams}"));
        }
    }

    if fail.is_empty() {
        Ok(())
    } else {
        Err(fail.join("; "))
    }
}

fn materialize_root(case: &Case, fixtures: &Path) -> Result<Option<PathBuf>, String> {
    let real_paths: Vec<&str> = case.files.iter().filter_map(|f| f.path.as_deref()).collect();
    if !real_paths.is_empty() {
        let all_exist = real_paths.iter().all(|p| fixtures.join(p).exists());
        if !all_exist {
            return Ok(None);
        }
        let first = fixtures.join(real_paths[0]);
        let parent = first.parent().ok_or("real path has no parent")?.to_path_buf();
        return Ok(Some(parent));
    }
    let root_dir = fixtures.join("cache").join("work").join(&case.id);
    std::fs::create_dir_all(&root_dir).map_err(|e| e.to_string())?;
    let mut payload: std::collections::HashMap<String, (Vec<u8>, String)> = std::collections::HashMap::new();
    for f in &case.files {
        let dest = root_dir.join(&f.name);
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let (bytes, base) = match &f.same_blob {
            Some(tok) => payload
                .entry(tok.clone())
                .or_insert_with(|| {
                    let b = synthesize(f, &derive_basename(&f.name));
                    (b, derive_basename(&f.name))
                })
                .to_owned(),
            None => {
                let base = f.known_basename.clone().unwrap_or_else(|| derive_basename(&f.name));
                (synthesize(f, &base), base)
            }
        };
        let _ = &base;
        std::fs::write(&dest, &bytes).map_err(|e| e.to_string())?;
    }
    Ok(Some(root_dir))
}

fn write_gguf_kv(b: &mut Vec<u8>, key: &str, tag: u32, val: &[u8]) {
    b.extend_from_slice(&(key.len() as u64).to_le_bytes());
    b.extend_from_slice(key.as_bytes());
    b.extend_from_slice(&tag.to_le_bytes());
    b.extend_from_slice(val);
}

fn synthesize(f: &FiFile, basename: &str) -> Vec<u8> {
    let lower = f.name.to_lowercase();
    if lower.ends_with(".gguf") {
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(b"GGUF");
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        let mut kvs: Vec<(String, u32, Vec<u8>)> = Vec::new();
        let kv = |out: &mut Vec<(String, u32, Vec<u8>)>, k: &str, t: u32, v: Vec<u8>| out.push((k.to_string(), t, v));
        kv(&mut kvs, "general.architecture", 8, strv("qwen3"));
        if !basename.is_empty() {
            kv(&mut kvs, "general.basename", 8, strv(basename));
        }
        if let Some(t) = f.known_params_total {
            kv(&mut kvs, "__janus_params_total", 12, f64v(t));
        }
        if let Some(a) = f.known_params_active {
            kv(&mut kvs, "__janus_params_active", 12, f64v(a));
        }
        b.extend_from_slice(&(kvs.len() as u64).to_le_bytes());
        for (k, t, v) in kvs {
            write_gguf_kv(&mut b, &k, t, &v);
        }
        b
    } else if lower.ends_with(".safetensors") {
        let json: Vec<u8> = if f.metadata_empty {
            b"{}".to_vec()
        } else {
            serde_json::json!({
                "__metadata__": { "format": "pt", "basename": basename },
                "model.embed_tokens.weight": { "dtype": "F32", "shape": [128, 128] }
            })
            .to_string()
            .into_bytes()
        };
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(&(json.len() as u64).to_le_bytes());
        b.extend(json);
        b.extend(vec![0u8; 128 * 128]);
        b
    } else {
        b"janus-fixture".to_vec()
    }
}

fn strv(s: &str) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&(s.len() as u64).to_le_bytes());
    v.extend_from_slice(s.as_bytes());
    v
}

fn f64v(f: f64) -> Vec<u8> {
    f.to_le_bytes().to_vec()
}

fn derive_basename(name: &str) -> String {
    let stem = name.rsplit('/').next().unwrap_or(name);
    let stem = stem
        .strip_suffix(".gguf")
        .or_else(|| stem.strip_suffix(".safetensors"))
        .unwrap_or(stem);
    let mut s = stem.to_string();
    for p in crate::filename::PUBLISHERS {
        if let Some(idx) = s.to_lowercase().find(&p.to_lowercase()) {
            s = s[..idx].to_string();
        }
    }
    let quant_re = regex::Regex::new(r"(?i)[-_](q[0-8](_[0-9a-z]+)*|iq[1-4](_[a-z0-9]+)*|f16|f32|bf16)").unwrap();
    s = quant_re.replace_all(&s, "").to_string();
    for marker in ["-mmproj", "-lora", "-tokenizer", "-vision-projector", "-ofa", "-bge"] {
        if let Some(base) = s.strip_suffix(marker) {
            s = base.to_string();
        }
    }
    crate::filename::slug(&s)
}