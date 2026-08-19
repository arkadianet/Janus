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
    delete_after_first_scan: Vec<String>,
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
    #[serde(default)]
    json: Option<String>,
    #[serde(default)]
    ollama_digest: bool,
    #[serde(default)]
    parse_state: Option<String>,
    #[serde(default)]
    parse_error: Option<String>,
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
    #[serde(default)]
    files_gone: i64,
    #[serde(default)]
    missing_rows: i64,
    #[serde(default)]
    present_rows: i64,
    #[serde(default)]
    parse_state: Option<String>,
    #[serde(default)]
    kinds: Vec<String>,
    #[serde(default)]
    pickle_refused: bool,
    #[serde(default)]
    provenance_files: Vec<String>,
    #[serde(default)]
    trusted_digest: bool,
}

#[derive(Deserialize, Default)]
struct Given {
    #[serde(default)]
    declined: Option<Declined>,
    #[serde(default)]
    merge: Option<Merge>,
}

#[derive(Deserialize)]
struct Merge {
    src: String,
    target: String,
}

#[derive(Deserialize)]
struct Declined {
    family_a_key: String,
    family_b_key: String,
    algo_version: String,
}

pub fn run_all(fixtures: &Path) -> Vec<CaseReport> {
    let cases_dir = fixtures.join("cases");
    let rd = match std::fs::read_dir(&cases_dir) {
        Ok(rd) => rd,
        Err(e) => {
            return vec![CaseReport {
                id: "cases".into(),
                title: String::new(),
                status: Status::Fail,
                detail: format!("read_dir:{}: {e}", cases_dir.display()),
            }];
        }
    };
    let mut tomls = Vec::new();
    let mut reports = Vec::new();
    for e in rd {
        match e {
            Ok(e) => {
                let p = e.path();
                if p.extension().map(|x| x == "toml").unwrap_or(false) {
                    tomls.push(p);
                }
            }
            Err(e) => reports.push(CaseReport {
                id: "cases".into(),
                title: String::new(),
                status: Status::Fail,
                detail: format!("readdir entry: {e}"),
            }),
        }
    }
    tomls.sort();
    reports.extend(tomls.into_iter().map(|p| {
            let id = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let src = match std::fs::read_to_string(&p) {
                Ok(s) => s,
                Err(e) => {
                    return CaseReport {
                        id,
                        title: String::new(),
                        status: Status::Fail,
                        detail: format!("read:{e}"),
                    };
                }
            };
            match toml::from_str::<Case>(&src) {
                Ok(case) => run_one(&case, fixtures),
                Err(e) => CaseReport {
                    id,
                    title: String::new(),
                    status: Status::Fail,
                    detail: format!("parse:{e}"),
                },
            }
        }));
    reports
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

    let root_id = match store::root_add_opts(&conn, &case.id, root_dir.to_string_lossy().as_ref(), "internal", true) {
        Ok(id) => id,
        Err(e) => return report(case, Status::Fail, format!("root:{e}")),
    };

    let opts = scan::ScanOptions { quick: case.scan.quick };
    if let Err(e) = scan::scan_root(&conn, root_id, &opts) {
        return report(case, Status::Fail, format!("scan:{e}"));
    }

    if let Some(m) = &case.given.merge {
        if let Err(e) = store::merge_families(&conn, &m.src, &m.target) {
            return report(case, Status::Fail, format!("merge:{e}"));
        }
        if let Err(e) = scan::scan_root(&conn, root_id, &opts) {
            return report(case, Status::Fail, format!("rescan-after-merge:{e}"));
        }
    }

    let mut second_report: Option<scan::ScanReport> = None;
    if !case.delete_after_first_scan.is_empty() {
        for rel in &case.delete_after_first_scan {
            let p = root_dir.join(rel);
            if let Err(e) = std::fs::remove_file(&p) {
                return report(case, Status::Fail, format!("delete {}: {e}", p.display()));
            }
        }
        second_report = Some(match scan::scan_root(&conn, root_id, &opts) {
            Ok(r) => r,
            Err(e) => return report(case, Status::Fail, format!("rescan:{e}")),
        });
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

    match assert_expectations(case, &conn, root_id, second_report.as_ref()) {
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

fn assert_expectations(case: &Case, conn: &Connection, root_id: i64, second: Option<&scan::ScanReport>) -> Result<(), String> {
    let mut fail: Vec<String> = Vec::new();

    for f in &case.files {
        if case.delete_after_first_scan.contains(&f.name) {
            continue;
        }
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
        if let Some(want) = &f.parse_state {
            let rel = file_rel(f);
            let got: Option<String> = conn
                .query_row(
                    "SELECT parse_state FROM files WHERE root_id=?1 AND rel_path=?2",
                    params![root_id, rel],
                    |r| r.get(0),
                )
                .ok();
            if got.as_deref() != Some(want.as_str()) {
                fail.push(format!("file {} parse_state want {want} got {:?}", f.name, got));
            }
        }
        if let Some(want) = &f.parse_error {
            let rel = file_rel(f);
            let got: Option<String> = conn
                .query_row(
                    "SELECT parse_error FROM files WHERE root_id=?1 AND rel_path=?2",
                    params![root_id, rel],
                    |r| r.get(0),
                )
                .ok();
            if got.as_deref() != Some(want.as_str()) {
                fail.push(format!("file {} parse_error want {want} got {:?}", f.name, got));
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
    let sorted = |mut want: Vec<String>| {
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
        let mut declined_nagged = false;
        if let Some(d) = &case.given.declined {
            let a = &d.family_a_key;
            let b = &d.family_b_key;
            if nags.iter().any(|s| (s.a_key == *a && s.b_key == *b) || (s.a_key == *b && s.b_key == *a)) {
                declined_nagged = true;
                fail.push("resweep_nags want no nag for declined pair".to_string());
            }
        }
        if !declined_nagged && !nags.is_empty() {
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

    if case.expect.files_gone != 0 {
        let got = second.map(|r| r.files_gone as i64).unwrap_or(0);
        if got != case.expect.files_gone {
            fail.push(format!("files_gone want {} got {got}", case.expect.files_gone));
        }
    }
    if case.expect.missing_rows != 0 {
        let got = count(conn, &format!("SELECT COUNT(*) FROM files WHERE root_id={root_id} AND state='missing'"));
        if got != case.expect.missing_rows {
            fail.push(format!("missing_rows want {} got {got}", case.expect.missing_rows));
        }
    }
    if case.expect.present_rows != 0 {
        let got = count(conn, &format!("SELECT COUNT(*) FROM files WHERE root_id={root_id} AND state='present'"));
        if got != case.expect.present_rows {
            fail.push(format!("present_rows want {} got {got}", case.expect.present_rows));
        }
    }
    if let Some(hs) = &case.expect.parse_state {
        let got: String = conn
            .query_row("SELECT parse_state FROM files WHERE root_id=?1 LIMIT 1", [root_id], |r| r.get(0))
            .unwrap_or_default();
        if &got != hs {
            fail.push(format!("parse_state want {hs} got {got}"));
        }
    }
    if !case.expect.kinds.is_empty() {
        let mut got: Vec<String> = conn
            .prepare("SELECT DISTINCT kind FROM model_families ORDER BY kind")
            .and_then(|mut s| s.query_map([], |r| r.get(0)).map(|it| it.collect::<Result<Vec<String>, _>>()))
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default();
        got.sort();
        if got != sorted(case.expect.kinds.clone()) {
            fail.push(format!("kinds want {:?} got {:?}", case.expect.kinds, got));
        }
    }
    if case.expect.pickle_refused {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE parse_error='pickle_refused'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if n == 0 {
            fail.push("pickle_refused want parse_error=pickle_refused".to_string());
        }
    }
    if !case.expect.provenance_files.is_empty() {
        for rel in &case.expect.provenance_files {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM provenance_entries p
                     JOIN files f ON f.id=p.subject_id
                     WHERE p.subject_type='file' AND f.rel_path=?1",
                    [rel],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if n == 0 {
                fail.push(format!("provenance missing on {rel}"));
            }
        }
    }
    if case.expect.trusted_digest {
        let got: String = conn
            .query_row("SELECT blake3 FROM blobs LIMIT 1", [], |r| r.get(0))
            .unwrap_or_default();
        if !got.starts_with("sha256:") {
            fail.push(format!("trusted_digest want blake3 sha256:… got {got}"));
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
        let mut payload: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
        for f in &case.files {
            if let Some(p) = &f.path {
                let dest = fixtures.join(p);
                if let Some(dir) = dest.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                if dest.exists() {
                    continue;
                }
                let bytes = match &f.same_blob {
                    Some(tok) => payload
                        .entry(tok.clone())
                        .or_insert_with(|| {
                            let base = derive_basename(&f.name);
                            synthesize(f, &base)
                        })
                        .clone(),
                    None => {
                        let base = f.known_basename.clone().unwrap_or_else(|| derive_basename(&f.name));
                        synthesize(f, &base)
                    }
                };
                std::fs::write(&dest, &bytes).map_err(|e| e.to_string())?;
            }
        }
        let first = fixtures.join(real_paths[0]);
        let parent = first.parent().ok_or("real path has no parent")?.to_path_buf();
        return Ok(Some(parent));
    }
    let root_dir = fixtures.join("cache").join("work").join(&case.id);
    if root_dir.exists() {
        std::fs::remove_dir_all(&root_dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&root_dir).map_err(|e| e.to_string())?;
    let mut payload: std::collections::HashMap<String, (Vec<u8>, String)> = std::collections::HashMap::new();
    for f in &case.files {
        let dest = if f.ollama_digest {
            root_dir.join("blobs")
        } else {
            root_dir.join(&f.name)
        };
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
        let dest = if f.ollama_digest {
            let sha = sha256_hex(&bytes);
            let p = root_dir.join(format!("sha256-{sha}"));
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            p
        } else {
            dest
        };
        std::fs::write(&dest, &bytes).map_err(|e| e.to_string())?;
    }
    Ok(Some(root_dir))
}

fn file_rel(f: &FiFile) -> String {
    if f.ollama_digest {
        String::new()
    } else {
        f.name.clone()
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
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
        if f.quant_from == "header" {
            if let Some(q) = &f.expect_quant {
                if let Some(ft) = crate::parse::gguf::quant_to_ftype(q) {
                    kv(&mut kvs, "general.file_type", 4, ft.to_le_bytes().to_vec());
                }
            }
        }
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
                "model.embed_tokens.weight": {
                    "dtype": "F32",
                    "shape": [128, 128],
                    "data_offsets": [0, 65536]
                }
            })
            .to_string()
            .into_bytes()
        };
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(&(json.len() as u64).to_le_bytes());
        b.extend(json);
        b.extend(vec![0u8; 65536]);
        b
    } else if lower.ends_with(".json") {
        f.json.as_deref().unwrap_or("{}").as_bytes().to_vec()
    } else if f.ollama_digest {
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(b"GGUF");
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
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
    let size_re = regex::Regex::new(r"(?i)[-_](?:[0-9]+(?:\.[0-9]+)?b|a[0-9]+(?:\.[0-9]+)?b)").unwrap();
    s = size_re.replace_all(&s, "").to_string();
    crate::filename::slug(&s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_cases_pass() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures");
        let reports = run_all(&fixtures);
        let fail: Vec<String> = reports
            .iter()
            .filter(|r| r.status == Status::Fail)
            .map(|r| format!("{}: {}", r.id, r.detail))
            .collect();
        assert!(fail.is_empty(), "{}", fail.join("\n"));
    }
}