use crate::detect;
use crate::ev::{Format, Kind, Level, Role};
use crate::filename;
use crate::hash;
use crate::identity;
use crate::parse;
use crate::store;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

pub const GGUF_PREFIX_CAP: usize = 1024 * 1024;

pub struct ScanOptions {
    pub quick: bool,
}

#[derive(Debug, Default)]
pub struct ScanReport {
    pub root_offline: bool,
    pub files_seen: u64,
    pub files_new: u64,
    pub files_changed: u64,
    pub files_gone: u64,
    pub files_unsupported: u64,
    pub files_unverified: u64,
    pub families_new: u64,
    pub duplicates: u64,
    pub skipped_symlink_dirs: u64,
    pub skipped_non_utf8: u64,
    pub dirs_unreadable: u64,
    pub skipped_deep: u64,
}

const MAX_WALK_DEPTH: u32 = 64;

pub fn scan_root(conn: &Connection, root_id: i64, opts: &ScanOptions) -> Result<ScanReport, String> {
    let root = store::root_by_id(conn, root_id)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut report = ScanReport::default();
    let root_path = PathBuf::from(&root.path);
    if !root_path.is_dir() {
        store::root_probe(conn, &root, now);
        report.root_offline = true;
        return Ok(report);
    }
    store::root_probe(conn, &root, now);
    conn.execute("DROP TABLE IF EXISTS _seen", []).ok();
    conn.execute("CREATE TEMP TABLE _seen(root_id INTEGER NOT NULL, rel TEXT NOT NULL, PRIMARY KEY(root_id, rel))", [])
        .map_err(to_scan_err)?;
    walk(conn, &root, &root_path, "", 0, opts, &mut report)?;
    attach_companions(conn, root_id);
    let tx = conn.unchecked_transaction().map_err(to_scan_err)?;
    let gone = reconcile_missing(&*tx, root_id)?;
    report.files_gone += gone;
    tx.execute("DROP TABLE IF EXISTS _seen", []).ok();
    store::root_scan_done(&*tx, root_id, now);
    tx.commit().map_err(to_scan_err)?;
    Ok(report)
}

fn to_scan_err(e: rusqlite::Error) -> String {
    format!("scan:{e}")
}

fn file_alloc_id(path: &Path, m: &std::fs::Metadata) -> (i64, i64) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = path;
        (m.dev() as i64, m.ino() as i64)
    }
    #[cfg(windows)]
    {
        let _ = m;
        win_alloc_id(path).unwrap_or_else(|| (0, path_fallback_ino(path)))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = m;
        (0, path_fallback_ino(path))
    }
}

#[cfg(windows)]
fn win_alloc_id(path: &Path) -> Option<(i64, i64)> {
    use std::os::windows::io::AsRawHandle;
    let f = std::fs::File::open(path).ok()?;
    let mut info = ByHandleFileInformation::default();
    let ok = unsafe { GetFileInformationByHandle(f.as_raw_handle(), &mut info) };
    if ok == 0 {
        return None;
    }
    let ino = ((info.n_file_index_high as u64) << 32) | info.n_file_index_low as u64;
    Some((info.volume_serial as i64, ino as i64))
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct ByHandleFileInformation {
    file_attributes: u32,
    creation: [u32; 2],
    access: [u32; 2],
    write: [u32; 2],
    volume_serial: u32,
    size_high: u32,
    size_low: u32,
    links: u32,
    n_file_index_high: u32,
    n_file_index_low: u32,
}

#[cfg(windows)]
extern "system" {
    fn GetFileInformationByHandle(
        handle: std::os::windows::raw::HANDLE,
        info: *mut ByHandleFileInformation,
    ) -> i32;
}

#[cfg(not(unix))]
fn path_fallback_ino(path: &Path) -> i64 {
    let s = path.to_string_lossy();
    let mut h: i64 = 0xcbf2_9ce4_8422_2325u64 as i64;
    for b in s.as_bytes() {
        h = h.wrapping_mul(0x100_0000_01b3u64 as i64) ^ (*b as i64);
    }
    h
}

fn reconcile_missing(conn: &Connection, root_id: i64) -> Result<u64, String> {
    let gone: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM files f
              WHERE f.root_id=?1 AND f.state='present'
                AND NOT EXISTS (SELECT 1 FROM _seen s WHERE s.root_id=?1 AND s.rel=f.rel_path)",
            [root_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if gone == 0 {
        return Ok(0);
    }
    conn.execute(
        "UPDATE files SET state='missing', blob_id=NULL, hash_state='none'
          WHERE root_id=?1 AND state='present'
            AND NOT EXISTS (SELECT 1 FROM _seen s WHERE s.root_id=?1 AND s.rel=files.rel_path)",
        [root_id],
    )
    .map_err(to_scan_err)?;
    conn.execute(
        "DELETE FROM file_roles WHERE file_id IN (
           SELECT id FROM files WHERE root_id=?1 AND state='missing')",
        [root_id],
    )
    .map_err(to_scan_err)?;
    Ok(gone as u64)
}

fn walk(conn: &Connection, root: &store::RootRow, dir: &Path, prefix: &str, depth: u32, opts: &ScanOptions, report: &mut ScanReport) -> Result<(), String> {
    if depth > MAX_WALK_DEPTH {
        report.skipped_deep += 1;
        return Ok(());
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => {
            report.dirs_unreadable += 1;
            return Ok(());
        }
    };
    for entry in entries.flatten() {
        let name = match entry.file_name().into_string() {
            Ok(s) => s,
            Err(_) => {
                report.skipped_non_utf8 += 1;
                continue;
            }
        };
        let meta = match std::fs::symlink_metadata(entry.path()) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.file_type().is_symlink() {
            let points_to_dir = std::fs::metadata(entry.path()).map(|m| m.is_dir()).unwrap_or(false);
            if points_to_dir {
                report.skipped_symlink_dirs += 1;
                continue;
            }
        } else if meta.is_dir() {
            walk(conn, root, &entry.path(), &format!("{prefix}{name}/"), depth + 1, opts, report)?;
            continue;
        } else if !meta.is_file() {
            continue;
        }
        let rel = format!("{prefix}{name}");
        ingest_file(conn, root, &rel, opts, report)?;
    }
    Ok(())
}

fn ingest_file(conn: &Connection, root: &store::RootRow, rel: &str, opts: &ScanOptions, report: &mut ScanReport) -> Result<(), String> {
    report.files_seen += 1;
    conn.execute("INSERT OR IGNORE INTO _seen(root_id, rel) VALUES (?1, ?2)", rusqlite::params![root.id, rel])
        .map_err(to_scan_err)?;
    let full = PathBuf::from(&root.path).join(rel);
    let sm = match std::fs::symlink_metadata(&full) {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };
    let is_symlink = sm.file_type().is_symlink();
    let m = match std::fs::metadata(&full) {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };
    if !m.is_file() {
        return Ok(());
    }
    let size = m.len() as i64;
    let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
    let ctime = m.created().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or_else(|| mtime);
    let (dev, ino) = file_alloc_id(&full, &m);

    let existing = store::file_find(conn, root.id, rel);
    let symlink_target = if is_symlink {
        std::fs::read_link(&full).ok().map(|p| p.to_string_lossy().to_string())
    } else {
        None
    };

    let (blob_id, hash_state) = if is_symlink {
        (None, "none".to_string())
    } else {
        decide_hash(conn, existing.as_ref(), &full, size, opts)
    };
    if hash_state != "full" {
        report.files_unverified += 1;
    }

    let (parse_state, parse_error, candidate) = classify_file(&full, rel, is_symlink, report);

    let file_id = match store::file_upsert(
        conn,
        root.id,
        rel,
        size,
        mtime,
        ctime,
        dev,
        ino,
        !is_symlink,
        symlink_target.as_deref(),
        blob_id,
        &hash_state,
        &parse_state,
        parse_error.as_deref(),
    ) {
        Ok(id) => id,
        Err(_) => return Ok(()),
    };

    match existing.as_ref() {
        None => report.files_new += 1,
        Some(ex) if ex.size != size || ex.mtime != mtime || ex.ino != ino => {
            report.files_changed += 1;
        }
        Some(_) => {}
    }

    if let Some(cand) = candidate {
        persist_identity(conn, file_id, &cand, blob_id, &hash_state, report);
    }
    if let Some((repo, rev)) = filename::hf_cache_repo(rel) {
        store::provenance_put(conn, "file", file_id, "downloaded_from", "hf-cache", Some(&repo), Some(&rev));
    }
    Ok(())
}

fn classify_file(
    full: &Path,
    rel: &str,
    is_symlink: bool,
    report: &mut ScanReport,
) -> (String, Option<String>, Option<identity::Candidate>) {
    if is_symlink {
        return ("unsupported".into(), Some("symlink".into()), None);
    }
    if filename::is_partial(rel) {
        return ("partial".into(), None, None);
    }
    let format = match detect::detect(full) {
        Ok(f) => f,
        Err(_) => Format::Unknown,
    };
    if filename::is_model_index(rel) || format == Format::Diffusers {
        let parsed = parse::parse_prefix(full, &Format::Diffusers, GGUF_PREFIX_CAP);
        if parsed.parse_error.is_some() {
            report.files_unsupported += 1;
            return ("unsupported".into(), parsed.parse_error, None);
        }
        return ("ok".into(), None, Some(identity::identify(rel, &parsed)));
    }
    if filename::is_weight_index(rel) || filename::is_config_json(rel) {
        let parsed = parse::Parsed {
            format: Format::Unknown,
            general_name: None,
            basename: None,
            finetune: None,
            arch: None,
            params_total: None,
            params_active: None,
            context_len: None,
            file_type: None,
            quant_from_header: None,
            kind: None,
            parse_error: None,
        };
        return ("ok".into(), None, Some(identity::identify(rel, &parsed)));
    }
    match format {
        Format::Unknown => {
            report.files_unsupported += 1;
            ("unsupported".into(), Some("unsupported".into()), None)
        }
        Format::Pytorch => {
            if let Some(cfg) = parse::config::read_adjacent(full) {
                let mut parsed = parse::parse_prefix(full, &format, GGUF_PREFIX_CAP);
                parsed.parse_error = None;
                parse::apply_config(&mut parsed, &cfg);
                ("ok".into(), Some("pickle_refused".into()), Some(identity::identify(rel, &parsed)))
            } else {
                report.files_unsupported += 1;
                ("unsupported".into(), Some("pickle_refused".into()), None)
            }
        }
        _ => {
            let mut parsed = parse::parse_prefix(full, &format, GGUF_PREFIX_CAP);
            if let Some(cfg) = parse::config::read_adjacent(full) {
                parse::apply_config(&mut parsed, &cfg);
            }
            if parsed.parse_error.is_some() {
                report.files_unsupported += 1;
                ("unsupported".into(), parsed.parse_error.clone(), None)
            } else {
                ("ok".into(), None, Some(identity::identify(rel, &parsed)))
            }
        }
    }
}

fn decide_hash(
    conn: &Connection,
    _existing: Option<&store::ExistingFile>,
    full: &Path,
    size: i64,
    opts: &ScanOptions,
) -> (Option<i64>, String) {
    if let Some(sha) = hash::ollama_named_sha256(full) {
        let key = format!("sha256:{sha}");
        match store::blob_upsert(conn, &key, Some(&sha), size, None) {
            Ok(id) => return (Some(id), "full".to_string()),
            Err(_) => {}
        }
    }
    // Partial (head+tail) xxh3 is not enough to reuse a full digest: a middle
    // overwrite would keep a stale BLAKE3. Full scans always rehash; --quick
    // stays hash_state=none.
    if opts.quick {
        return (None, "none".to_string());
    }
    match hash::full_hash(full) {
        Ok((b3, s256, len, partial)) => {
            let partial = partial.to_string();
            match store::blob_upsert(conn, &b3, Some(&s256), len as i64, Some(&partial)) {
                Ok(id) => (Some(id), "full".to_string()),
                Err(_) => (None, "none".to_string()),
            }
        }
        Err(_) => (None, "none".to_string()),
    }
}

fn persist_identity(
    conn: &Connection,
    file_id: i64,
    cand: &identity::Candidate,
    blob_id: Option<i64>,
    hash_state: &str,
    report: &mut ScanReport,
) {
    let blob_family = blob_id.and_then(|id| store::family_for_blob(conn, id));
    let as_model = matches!(cand.role, Role::Weights | Role::Shard) || cand.kind.value == Kind::Diffusion;
    if as_model {
            let key = match &cand.family_key {
                Some(k) => k.clone(),
                None if cand.kind.value == Kind::Diffusion => {
                    identity::family_key(&cand.display_name.value, None, None, None)
                }
                None => return,
            };
            let source = level_source(cand.display_name.level);
            let family_id = if let Some(id) = blob_family {
                id
            } else {
                match store::family_resolve(conn, &key) {
                Some(id) => id,
                None => match store::family_insert(
                    conn,
                    &key,
                    Some(cand.display_name.value.as_str()),
                    cand.arch.as_ref().map(|a| a.value.as_str()),
                    cand.params_total.as_ref().map(|p| p.value),
                    cand.params_active.as_ref().map(|p| p.value),
                    cand.context_len.as_ref().map(|c| c.value),
                    cand.kind.value.as_str(),
                ) {
                    Ok(id) => {
                        report.families_new += 1;
                        id
                    }
                    Err(_) => return,
                },
                }
            };
            if cand.arch.is_some() || cand.params_total.is_some() || cand.context_len.is_some() {
                store::evidence_put(conn, "family", family_id, "name", &cand.display_name.value, level_str(cand.display_name.level), source);
            }
            if let Some(a) = &cand.arch {
                store::evidence_put(conn, "family", family_id, "arch", &a.value, "known", "content");
            }
            if let Some(p) = &cand.params_total {
                store::evidence_put(conn, "family", family_id, "params_total", &identity::round1(p.value), "known", "content");
            }
            store::evidence_put(conn, "family", family_id, "kind", cand.kind.value.as_str(), level_str(cand.kind.level), source);

            let rev_label = match (blob_id, hash_state) {
                (Some(id), "full") => store::blob_find(conn, id)
                    .map(|b| format!("local:{}", b.blake3))
                    .unwrap_or_else(|| "local:none".to_string()),
                _ => "local:none".to_string(),
            };
            let revision_id = match store::revision_find_or_insert(conn, family_id, &rev_label) {
                Ok(id) => id,
                Err(_) => return,
            };
            let qs = cand.quant.value.as_str();
            let variant_id = match store::variant_find_or_insert(
                conn,
                family_id,
                revision_id,
                qs,
                cand.quant_raw.as_ref().map(|r| r.value.as_str()),
                cand.format.as_str(),
                cand.subflavour.value.as_str(),
                cand.publisher.value.as_str(),
            ) {
                Ok(id) => id,
                Err(_) => return,
            };
            store::file_role_put(conn, file_id, Some(variant_id), Some(family_id), cand.role.as_str());
            store::evidence_put(conn, "variant", variant_id, "quant", qs, level_str(cand.quant.level), level_source(cand.quant.level));
            store::evidence_put(conn, "variant", variant_id, "publisher", &cand.publisher.value, level_str(cand.publisher.level), "filename");
            store::evidence_put(conn, "variant", variant_id, "subflavour", &cand.subflavour.value, level_str(cand.subflavour.level), "filename");
            return;
    }
    let family_id = blob_family.or_else(|| cand.family_key.as_ref().and_then(|k| store::family_resolve(conn, k)));
    store::file_role_put(conn, file_id, None, family_id, cand.role.as_str());
    store::evidence_put(conn, "file", file_id, "role", cand.role.as_str(), "inferred", "filename");
}

fn attach_companions(conn: &Connection, root_id: i64) {
    let mut stmt = match conn.prepare(
        "SELECT f.id, f.rel_path FROM files f
         JOIN file_roles fr ON fr.file_id=f.id
         WHERE f.root_id=?1 AND fr.family_id IS NULL AND fr.variant_id IS NULL
           AND fr.role IN ('config','lora','mmproj','tokenizer','sidecar')",
    ) {
        Ok(s) => s,
        Err(_) => return,
    };
    let rows: Vec<(i64, String)> = stmt
        .query_map([root_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .ok()
        .map(|it| it.flatten().collect())
        .unwrap_or_default();
    drop(stmt);
    for (file_id, rel) in rows {
        let parent = rel.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
        let like = if parent.is_empty() { "%".to_string() } else { format!("{parent}/%") };
        let fam: Option<i64> = conn
            .query_row(
                "SELECT COALESCE(v.family_id, fr.family_id) FROM files f
                 JOIN file_roles fr ON fr.file_id=f.id
                 LEFT JOIN model_variants v ON v.id=fr.variant_id
                 WHERE f.root_id=?1 AND f.id!=?2 AND (v.family_id IS NOT NULL OR fr.family_id IS NOT NULL)
                   AND (f.rel_path LIKE ?3 OR (?4 = '' AND instr(f.rel_path, '/')=0))
                 LIMIT 1",
                rusqlite::params![root_id, file_id, like, parent],
                |r| r.get(0),
            )
            .ok();
        if let Some(fid) = fam {
            conn.execute("UPDATE file_roles SET family_id=?1 WHERE file_id=?2", rusqlite::params![fid, file_id])
                .ok();
        }
    }
}

fn level_str(l: Level) -> &'static str {
    match l {
        Level::Known => "known",
        Level::Detected => "detected",
        Level::Inferred => "inferred",
        Level::External => "external",
        Level::Manual => "manual",
    }
}

fn level_source(l: Level) -> &'static str {
    match l {
        Level::Known => "content",
        Level::Detected => "structure",
        Level::Inferred => "filename",
        Level::External => "external",
        Level::Manual => "user",
    }
}