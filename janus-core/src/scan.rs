use crate::detect;
use crate::ev::{Format, Level, Role};
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
    pub files_unsupported: u64,
    pub files_unverified: u64,
    pub families_new: u64,
    pub duplicates: u64,
    pub skipped_symlink_dirs: u64,
}

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
    walk(conn, &root, &root_path, "", opts, &mut report);
    store::root_scan_done(conn, root_id, now);
    Ok(report)
}

fn walk(conn: &Connection, root: &store::RootRow, dir: &Path, prefix: &str, opts: &ScanOptions, report: &mut ScanReport) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let meta = match std::fs::symlink_metadata(entry.path()) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_dir() {
            if meta.file_type().is_symlink() {
                report.skipped_symlink_dirs += 1;
            } else {
                walk(conn, root, &entry.path(), &format!("{prefix}{name}/"), opts, report);
            }
            continue;
        }
        if !meta.is_file() && !meta.file_type().is_symlink() {
            continue;
        }
        let rel = format!("{prefix}{name}");
        ingest_file(conn, root, &rel, opts, report);
    }
}

fn ingest_file(conn: &Connection, root: &store::RootRow, rel: &str, opts: &ScanOptions, report: &mut ScanReport) {
    report.files_seen += 1;
    let full = PathBuf::from(&root.path).join(rel);
    let sm = match std::fs::symlink_metadata(&full) {
        Ok(m) => m,
        Err(_) => return,
    };
    let is_symlink = sm.file_type().is_symlink();
    let m = match std::fs::metadata(&full) {
        Ok(m) => m,
        Err(_) => return,
    };
    if !m.is_file() {
        return;
    }
    let size = m.len() as i64;
    let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
    let ctime = m.created().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or_else(|| mtime);
    #[cfg(unix)]
    let (dev, ino) = {
        use std::os::unix::fs::MetadataExt;
        (m.dev() as i64, m.ino() as i64)
    };
    #[cfg(not(unix))]
    let (dev, ino) = (0i64, 0i64);

    let existing = store::file_find(conn, root.id, rel);
    let is_new = existing.is_none();

    let symlink_target = if is_symlink {
        std::fs::read_link(&full).ok().map(|p| p.to_string_lossy().to_string())
    } else {
        None
    };

    let (blob_id, hash_state) = if is_symlink {
        (None, "none".to_string())
    } else {
        decide_hash(conn, existing.as_ref(), &full, opts)
    };
    if hash_state != "full" {
        report.files_unverified += 1;
    }

    let (parse_state, parse_error, _format, _parsed, candidate) = if is_symlink {
        ("unsupported".to_string(), Some("symlink".to_string()), Format::Unknown, None::<parse::Parsed>, None)
    } else {
        let format = match detect::detect(&full) {
            Ok(f) => f,
            Err(_) => Format::Unknown,
        };
        match format {
            Format::Unknown => {
                report.files_unsupported += 1;
                ("unsupported".to_string(), Some("unsupported".to_string()), format, None, None)
            }
            _ => {
                let parsed = parse::parse_prefix(&full, &format, GGUF_PREFIX_CAP);
                if parsed.parse_error.is_some() {
                    report.files_unsupported += 1;
                    ("unsupported".to_string(), parsed.parse_error.clone(), format, None, None)
                } else {
                    let cand = identity::identify(rel, &parsed);
                    ("ok".to_string(), None, format, None, Some(cand))
                }
            }
        }
    };

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
        Err(_) => return,
    };

    if is_new {
        report.files_new += 1;
    } else {
        report.files_changed += 1;
    }

    if let Some(cand) = candidate {
        persist_identity(conn, file_id, &cand, blob_id, &hash_state, report);
    }
}

fn decide_hash(
    conn: &Connection,
    existing: Option<&store::ExistingFile>,
    full: &Path,
    opts: &ScanOptions,
) -> (Option<i64>, String) {
    let reuses = |ex: &store::ExistingFile| -> bool {
        if ex.hash_state.as_deref() != Some("full") {
            return false;
        }
        let Some(blob) = ex.blob_id.and_then(|id| store::blob_find(conn, id)) else {
            return false;
        };
        let Some(stored_partial) = blob.partial else { return false; };
        match hash::partial_hash(full) {
            Ok((p, _)) => Some(p.to_string()) == Some(stored_partial),
            Err(_) => false,
        }
    };

    if opts.quick {
        if let Some(ex) = existing {
            if reuses(ex) {
                return (ex.blob_id, "full".to_string());
            }
        }
        return (None, "none".to_string());
    }

    if let Some(ex) = existing {
        if reuses(ex) {
            return (ex.blob_id, "full".to_string());
        }
    }
    match hash::full_hash(full) {
        Ok((b3, s256, len)) => {
            let partial = hash::partial_hash(full).ok().map(|(p, _)| p.to_string());
            let id = store::blob_upsert(conn, &b3, Some(&s256), len as i64, partial.as_deref()).unwrap_or(0);
            (Some(id), "full".to_string())
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
    match cand.role {
        Role::Weights | Role::Shard => {
            let Some(key) = &cand.family_key else {
                return;
            };
            let source = level_source(cand.display_name.level);
            let family_id = match store::family_find(conn, key) {
                Some(id) => id,
                None => {
                    let id = store::family_insert(
                        conn,
                        key,
                        Some(cand.display_name.value.as_str()),
                        cand.arch.as_ref().map(|a| a.value.as_str()),
                        cand.params_total.as_ref().map(|p| p.value),
                        cand.params_active.as_ref().map(|p| p.value),
                        cand.context_len.as_ref().map(|c| c.value),
                        cand.kind.value.as_str(),
                    )
                    .unwrap_or(0);
                    report.families_new += 1;
                    id
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
                (Some(_), "full") => "local:none",
                _ => "local:none",
            };
            let revision_id = store::revision_find_or_insert(conn, family_id, rev_label).unwrap_or(0);
            let qs = cand.quant.value.as_str();
            let variant_id = store::variant_find_or_insert(
                conn,
                family_id,
                revision_id,
                qs,
                cand.quant_raw.as_ref().map(|r| r.value.as_str()),
                cand.format.as_str(),
                cand.subflavour.value.as_str(),
                cand.publisher.value.as_str(),
            )
            .unwrap_or(0);
            store::file_role_put(conn, file_id, Some(variant_id), None, cand.role.as_str());
            store::evidence_put(conn, "variant", variant_id, "quant", qs, level_str(cand.quant.level), level_source(cand.quant.level));
            store::evidence_put(conn, "variant", variant_id, "publisher", &cand.publisher.value, level_str(cand.publisher.level), "filename");
            store::evidence_put(conn, "variant", variant_id, "subflavour", &cand.subflavour.value, level_str(cand.subflavour.level), "filename");
        }
        role @ (Role::Mmproj | Role::Lora | Role::Tokenizer | Role::Config | Role::Sidecar) => {
            let key = cand.family_key.clone();
            let family_id = key.as_ref().and_then(|k| store::family_find(conn, k));
            store::file_role_put(conn, file_id, None, family_id, role.as_str());
            store::evidence_put(conn, "file", file_id, "role", role.as_str(), "inferred", "filename");
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