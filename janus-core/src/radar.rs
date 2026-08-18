//! Radar: profiles + monitors + wanted. Read-only — never downloads bytes.

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::availability::{self, AvailabilityProvider, RemoteFile};
use crate::filename;
use crate::profile::{self, QualityProfile};
use crate::store;

pub const PRIVACY_NOTICE: &str = "A sweep sends repository id, revision, and remote file names to Hugging Face so Janus can list what exists. Model weights do not leave this machine. HF_TOKEN is sent only if you set it. Scan never uses the network.";

#[derive(Debug, Clone, Default)]
pub struct SweepOpts {
    pub opt_in: bool,
    pub families: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SweepReport {
    pub monitors: usize,
    pub upserted: usize,
    pub open: usize,
    pub skipped_have_bytes: usize,
    pub satisfied: usize,
    pub privacy_notice: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonitorRow {
    pub id: i64,
    pub family_id: i64,
    pub family: String,
    pub variant_id: Option<i64>,
    pub profile_id: i64,
    pub profile: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WantedRow {
    pub id: i64,
    pub monitor_id: Option<i64>,
    pub family: String,
    pub family_id: Option<i64>,
    pub remote_key: String,
    pub provider: String,
    pub repo: String,
    pub revision: String,
    pub filename: String,
    pub size: Option<i64>,
    pub sha256: Option<String>,
    pub status: String,
    pub local_root: Option<String>,
    pub local_present: bool,
    pub note: String,
}

#[derive(Debug, Clone, Default)]
pub struct WantedFilter {
    pub status: Option<String>,
    pub open: bool,
    pub have_offline: bool,
}

pub fn monitor_add(
    conn: &Connection,
    family_id: i64,
    variant_id: Option<i64>,
    profile_id: i64,
    enabled: bool,
) -> Result<i64, String> {
    if variant_id.is_some() {
        let vf: Option<i64> = conn
            .query_row(
                "SELECT family_id FROM model_variants WHERE id=?1",
                [variant_id.unwrap()],
                |r| r.get(0),
            )
            .ok();
        if vf != Some(family_id) {
            return Err("radar.variant_family_mismatch".into());
        }
    }
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM model_families WHERE id=?1", [family_id], |r| r.get(0))
        .map_err(store::to_err)?;
    if n == 0 {
        return Err("identity.not_found".into());
    }
    let p: i64 = conn
        .query_row("SELECT COUNT(*) FROM quality_profiles WHERE id=?1", [profile_id], |r| r.get(0))
        .map_err(store::to_err)?;
    if p == 0 {
        return Err("identity.not_found".into());
    }
    conn.execute(
        "INSERT INTO monitors (family_id, variant_id, profile_id, enabled) VALUES (?1,?2,?3,?4)",
        params![family_id, variant_id, profile_id, enabled as i64],
    )
    .map_err(store::to_err)?;
    Ok(conn.last_insert_rowid())
}

pub fn monitor_rm(conn: &Connection, id: i64) -> Result<(), String> {
    let n = conn.execute("DELETE FROM monitors WHERE id=?1", [id]).map_err(store::to_err)?;
    if n == 0 {
        return Err("identity.not_found".into());
    }
    Ok(())
}

pub fn monitor_list(conn: &Connection) -> Result<Vec<MonitorRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.family_id, COALESCE(f.name, f.family_key), m.variant_id, m.profile_id,
                    p.name, m.enabled
               FROM monitors m
               JOIN model_families f ON f.id=m.family_id
               JOIN quality_profiles p ON p.id=m.profile_id
              ORDER BY m.id",
        )
        .map_err(store::to_err)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(MonitorRow {
                id: r.get(0)?,
                family_id: r.get(1)?,
                family: r.get(2)?,
                variant_id: r.get(3)?,
                profile_id: r.get(4)?,
                profile: r.get(5)?,
                enabled: r.get::<_, i64>(6)? == 1,
            })
        })
        .map_err(store::to_err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(store::to_err)
}

pub fn sweep(conn: &Connection, provider: &dyn AvailabilityProvider, opts: &SweepOpts) -> Result<SweepReport, String> {
    if !opts.opt_in {
        return Err("network.disabled".into());
    }
    let monitors = monitor_list(conn)?;
    let mut report = SweepReport {
        monitors: 0,
        upserted: 0,
        open: 0,
        skipped_have_bytes: 0,
        satisfied: 0,
        privacy_notice: PRIVACY_NOTICE.into(),
    };
    for mon in monitors {
        if !mon.enabled {
            continue;
        }
        if !opts.families.is_empty() {
            let needle = opts.families.iter().any(|f| {
                f == &mon.family || store::family_find_id(conn, f) == Some(mon.family_id)
            });
            if !needle {
                continue;
            }
        }
        report.monitors += 1;
        let profile = profile::get(conn, mon.profile_id)?.spec;
        let repo = family_repo(conn, mon.family_id);
        let Some(repo) = repo else {
            continue;
        };
        let listed = provider.list(&repo, None)?;
        let selected = select_files(&listed, &profile, mon.variant_id.and_then(|vid| variant_hole(conn, vid)));
        for file in selected {
            let status = classify(conn, mon.family_id, &file, &profile, mon.variant_id);
            upsert_wanted(conn, mon.id, &file, &status)?;
            report.upserted += 1;
            match status.status.as_str() {
                "open" => report.open += 1,
                "skipped_have_bytes" => report.skipped_have_bytes += 1,
                "satisfied" => report.satisfied += 1,
                _ => {}
            }
        }
    }
    Ok(report)
}

struct Classified {
    status: String,
    local_blob_id: Option<i64>,
    local_root_id: Option<i64>,
}

struct Hole {
    quant: String,
    format: String,
    publisher: String,
}

fn family_repo(conn: &Connection, family_id: i64) -> Option<String> {
    conn.query_row(
        "SELECT repo FROM provenance_entries
          WHERE subject_type='family' AND subject_id=?1 AND repo IS NOT NULL AND repo != ''
          ORDER BY id DESC LIMIT 1",
        [family_id],
        |r| r.get(0),
    )
    .ok()
    .or_else(|| {
        conn.query_row(
            "SELECT payload_json FROM enrichments
              WHERE subject_type='family' AND subject_id=?1 AND provider='hf'
              ORDER BY id DESC LIMIT 1",
            [family_id],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
        .and_then(|v| v.get("repo").and_then(|r| r.as_str()).map(|s| s.to_string()))
    })
}

fn variant_hole(conn: &Connection, variant_id: i64) -> Option<Hole> {
    conn.query_row(
        "SELECT quant, format, publisher FROM model_variants WHERE id=?1",
        [variant_id],
        |r| {
            Ok(Hole {
                quant: r.get(0)?,
                format: r.get(1)?,
                publisher: r.get(2)?,
            })
        },
    )
    .ok()
}

fn select_files(listed: &[RemoteFile], profile: &QualityProfile, hole: Option<Hole>) -> Vec<RemoteFile> {
    let mut eligible: Vec<RemoteFile> = listed
        .iter()
        .filter(|f| eligible_file(f, profile, hole.as_ref()))
        .cloned()
        .collect();
    if !profile.publishers.is_empty() {
        let chosen = profile.publishers.iter().find(|p| {
            eligible.iter().any(|f| f.publisher.eq_ignore_ascii_case(p))
        });
        if let Some(pub_name) = chosen {
            eligible.retain(|f| f.publisher.eq_ignore_ascii_case(pub_name));
        } else {
            eligible.clear();
        }
    }
    eligible
}

fn eligible_file(f: &RemoteFile, profile: &QualityProfile, hole: Option<&Hole>) -> bool {
    let fmt = availability::format_of(&f.filename);
    if !profile.formats.is_empty() && !profile.formats.iter().any(|x| x.eq_ignore_ascii_case(&fmt)) {
        return false;
    }
    let quant = filename::quant_tag(&filename::stem(&f.filename)).unwrap_or_else(|| "unknown".into());
    if let Some(h) = hole {
        if !quant.eq_ignore_ascii_case(&h.quant) {
            return false;
        }
        if h.format != "unknown" && !fmt.eq_ignore_ascii_case(&h.format) {
            return false;
        }
        if h.publisher != "unknown" && !f.publisher.eq_ignore_ascii_case(&h.publisher) {
            return false;
        }
    } else if !profile.quants.is_empty() && !profile.quants.iter().any(|q| q.eq_ignore_ascii_case(&quant)) {
        return false;
    }
    if let Some(max) = profile.max_bytes {
        if let Some(sz) = f.size {
            if sz as u64 > max {
                return false;
            }
        }
    }
    let name = f.filename.to_ascii_lowercase();
    if profile.exclude_name.iter().any(|tok| !tok.is_empty() && name.contains(&tok.to_ascii_lowercase())) {
        return false;
    }
    true
}

fn classify(
    conn: &Connection,
    family_id: i64,
    file: &RemoteFile,
    profile: &QualityProfile,
    variant_id: Option<i64>,
) -> Classified {
    if let Some(sha) = file.sha256.as_deref() {
        if let Some((blob_id, root_id)) = have_verified_sha256(conn, sha) {
            return Classified {
                status: "skipped_have_bytes".into(),
                local_blob_id: Some(blob_id),
                local_root_id: Some(root_id),
            };
        }
    }
    if variant_id.is_none() {
        if let Some(cutoff) = &profile.cutoff {
            if revision_meets_cutoff(conn, family_id, &file.revision, cutoff) {
                return Classified { status: "satisfied".into(), local_blob_id: None, local_root_id: None };
            }
        }
    } else if let Some(vid) = variant_id {
        if variant_present_in_revision(conn, vid, &file.revision) {
            return Classified { status: "satisfied".into(), local_blob_id: None, local_root_id: None };
        }
    }
    Classified { status: "open".into(), local_blob_id: None, local_root_id: None }
}

/// Verified ownership: full hash (or trusted digest stored as full) + sha256 match.
pub fn have_verified_sha256(conn: &Connection, sha256: &str) -> Option<(i64, i64)> {
    let sha = sha256.trim_start_matches("sha256:").to_ascii_lowercase();
    conn.query_row(
        "SELECT b.id, f.root_id FROM blobs b
         JOIN files f ON f.blob_id=b.id
         WHERE lower(b.sha256)=?1 AND f.hash_state='full' AND f.blob_id IS NOT NULL
         LIMIT 1",
        [sha],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .ok()
}

fn revision_meets_cutoff(conn: &Connection, family_id: i64, revision: &str, cutoff: &str) -> bool {
    let Ok(mut stmt) = conn.prepare(
        "SELECT v.quant FROM model_variants v
         JOIN model_revisions r ON r.id=v.revision_id
         WHERE v.family_id=?1 AND r.rev_label=?2",
    ) else {
        return false;
    };
    let Ok(rows) = stmt.query_map(params![family_id, revision], |r| r.get::<_, String>(0)) else {
        return false;
    };
    let quants: Vec<String> = rows.flatten().collect();
    quants.iter().any(|q| profile::meets_cutoff(q, cutoff))
}

fn variant_present_in_revision(conn: &Connection, variant_id: i64, revision: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM model_variants v
         JOIN model_revisions r ON r.id=v.revision_id
         WHERE v.id=?1 AND r.rev_label=?2",
        params![variant_id, revision],
        |r| r.get::<_, i64>(0),
    )
    .ok()
    .map(|n| n > 0)
    .unwrap_or(false)
}

fn upsert_wanted(conn: &Connection, monitor_id: i64, file: &RemoteFile, status: &Classified) -> Result<(), String> {
    let key = availability::remote_key("hf", &file.repo, &file.revision, &file.filename);
    conn.execute(
        "INSERT INTO wanted_items (monitor_id, remote_key, provider, repo, revision, filename, size, sha256, status, local_blob_id, local_root_id)
         VALUES (?1,?2,'hf',?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(remote_key) DO UPDATE SET
           monitor_id=excluded.monitor_id,
           size=excluded.size,
           sha256=excluded.sha256,
           status=CASE WHEN wanted_items.status='dismissed' THEN wanted_items.status ELSE excluded.status END,
           local_blob_id=excluded.local_blob_id,
           local_root_id=excluded.local_root_id",
        params![
            monitor_id,
            key,
            file.repo,
            file.revision,
            file.filename,
            file.size,
            file.sha256,
            status.status,
            status.local_blob_id,
            status.local_root_id
        ],
    )
    .map_err(store::to_err)?;
    Ok(())
}

pub fn wanted(conn: &Connection, filter: &WantedFilter) -> Result<Vec<WantedRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT w.id, w.monitor_id, COALESCE(f.name, f.family_key, w.repo), m.family_id,
                    w.remote_key, w.provider, w.repo, w.revision, w.filename, w.size, w.sha256,
                    w.status, r.name, COALESCE(r.present,0)
               FROM wanted_items w
               LEFT JOIN monitors m ON m.id=w.monitor_id
               LEFT JOIN model_families f ON f.id=m.family_id
               LEFT JOIN storage_roots r ON r.id=w.local_root_id
              ORDER BY w.id",
        )
        .map_err(store::to_err)?;
    let rows = stmt
        .query_map([], |r| {
            let status: String = r.get(11)?;
            let local_root: Option<String> = r.get(12)?;
            let present = r.get::<_, i64>(13)? == 1;
            let note = match (status.as_str(), local_root.as_deref(), present) {
                ("skipped_have_bytes", Some(name), false) => format!("{name} (offline)"),
                ("skipped_have_bytes", Some(name), true) => name.to_string(),
                _ => String::new(),
            };
            Ok(WantedRow {
                id: r.get(0)?,
                monitor_id: r.get(1)?,
                family: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                family_id: r.get(3)?,
                remote_key: r.get(4)?,
                provider: r.get(5)?,
                repo: r.get(6)?,
                revision: r.get(7)?,
                filename: r.get(8)?,
                size: r.get(9)?,
                sha256: r.get(10)?,
                status,
                local_root,
                local_present: present,
                note,
            })
        })
        .map_err(store::to_err)?;
    let mut out = rows.collect::<Result<Vec<_>, _>>().map_err(store::to_err)?;
    if let Some(st) = &filter.status {
        out.retain(|w| w.status == *st);
    }
    if filter.open {
        out.retain(|w| w.status == "open");
    }
    if filter.have_offline {
        out.retain(|w| w.status == "skipped_have_bytes" && !w.local_present);
    }
    Ok(out)
}

pub fn wanted_open_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM wanted_items WHERE status='open'", [], |r| r.get(0))
        .unwrap_or(0)
}

pub fn wanted_by_id(conn: &Connection, id: i64) -> Result<WantedRow, String> {
    wanted(conn, &WantedFilter::default())?
        .into_iter()
        .find(|w| w.id == id)
        .ok_or_else(|| "identity.not_found".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::availability::{MemoryProvider, RemoteFile};
    use crate::db;

    fn mem() -> Connection {
        let c = db::open(None).unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    fn setup_monitor(c: &Connection) -> (i64, i64) {
        let fam = store::family_insert(c, "qwen|llama|t8|a8", Some("Qwen"), Some("llama"), Some(8.0), None, None, "llm").unwrap();
        store::provenance_put(c, "family", fam, "seen_in", "hf", Some("acme/Qwen-GGUF"), Some("main"));
        let pid = profile::ensure_default(c).unwrap();
        let mid = monitor_add(c, fam, None, pid, true).unwrap();
        (fam, mid)
    }

    fn remote(rev: &str, name: &str, publisher: &str, sha: Option<String>) -> RemoteFile {
        RemoteFile {
            repo: "acme/Qwen-GGUF".into(),
            revision: rev.into(),
            filename: name.into(),
            size: Some(1000),
            sha256: sha,
            publisher: publisher.into(),
        }
    }

    fn sweep_ok(c: &Connection, files: Vec<RemoteFile>) -> SweepReport {
        let p = MemoryProvider { files };
        sweep(c, &p, &SweepOpts { opt_in: true, families: vec![] }).unwrap()
    }

    #[test]
    fn variant_must_belong_to_family() {
        let c = mem();
        let a = store::family_insert(&c, "a|llama|t8|a8", Some("A"), None, None, None, None, "llm").unwrap();
        let b = store::family_insert(&c, "b|llama|t8|a8", Some("B"), None, None, None, None, "llm").unwrap();
        let rev = store::revision_find_or_insert(&c, b, "local:none").unwrap();
        let vid = store::variant_find_or_insert(&c, b, rev, "Q5_K_M", None, "gguf", "unknown", "bartowski").unwrap();
        let pid = profile::ensure_default(&c).unwrap();
        let err = monitor_add(&c, a, Some(vid), pid, true).unwrap_err();
        assert_eq!(err, "radar.variant_family_mismatch");
    }

    #[test]
    fn sweep_without_opt_in_is_disabled() {
        let c = mem();
        setup_monitor(&c);
        let p = MemoryProvider { files: vec![] };
        let err = sweep(&c, &p, &SweepOpts { opt_in: false, families: vec![] }).unwrap_err();
        assert_eq!(err, "network.disabled");
        assert!(PRIVACY_NOTICE.contains("Hugging Face"));
        assert!(PRIVACY_NOTICE.contains("repository id"));
        assert!(PRIVACY_NOTICE.contains("weights do not leave"));
    }

    #[test]
    fn publisher_highest_preference_only() {
        let c = mem();
        setup_monitor(&c);
        sweep_ok(
            &c,
            vec![
                remote("main", "bartowski-Qwen-Q4_K_M.gguf", "bartowski", Some("aa".repeat(32))),
                remote("main", "Qwen-Q4_K_M.gguf", "official", Some("bb".repeat(32))),
                remote("main", "mradermacher-Qwen-Q4_K_M.gguf", "mradermacher", Some("cc".repeat(32))),
            ],
        );
        let rows = wanted(&c, &WantedFilter::default()).unwrap();
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].filename, "bartowski-Qwen-Q4_K_M.gguf");
        assert_eq!(rows[0].status, "open");
    }

    #[test]
    fn old_cutoff_cannot_hide_new_revision() {
        let c = mem();
        let (fam, _) = setup_monitor(&c);
        c.execute(
            "INSERT INTO model_revisions (family_id, rev_kind, rev_label) VALUES (?1,'commit','oldrev')",
            [fam],
        )
        .unwrap();
        let old_id: i64 = c
            .query_row("SELECT id FROM model_revisions WHERE rev_label='oldrev'", [], |r| r.get(0))
            .unwrap();
        store::variant_find_or_insert(&c, fam, old_id, "Q4_K_M", None, "gguf", "unknown", "bartowski").unwrap();
        sweep_ok(
            &c,
            vec![
                remote("oldrev", "Qwen-Q4_K_M.gguf", "bartowski", Some("11".repeat(32))),
                remote("newrev", "Qwen-Q4_K_M.gguf", "bartowski", Some("22".repeat(32))),
            ],
        );
        let rows = wanted(&c, &WantedFilter::default()).unwrap();
        let old = rows.iter().find(|w| w.revision == "oldrev").expect("old");
        let new = rows.iter().find(|w| w.revision == "newrev").expect("new");
        assert_eq!(old.status, "satisfied");
        assert_eq!(new.status, "open");
    }

    #[test]
    fn unverified_file_does_not_satisfy_have_bytes() {
        let c = mem();
        setup_monitor(&c);
        let dir = std::env::temp_dir().join(format!("janus-radar-unverified-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let root = store::root_add(&c, "models", dir.to_str().unwrap(), "internal").unwrap();
        let sha = "33".repeat(32);
        let blob = store::blob_upsert(&c, "blake-unverified", Some(&sha), 1000, None).unwrap();
        store::file_upsert(&c, root, "Qwen-Q4_K_M.gguf", 1000, 0, 0, 0, 1, true, None, Some(blob), "none", "ok", None)
            .unwrap();
        sweep_ok(
            &c,
            vec![remote("main", "bartowski-Qwen-Q4_K_M.gguf", "bartowski", Some(sha.clone()))],
        );
        let rows = wanted(&c, &WantedFilter::default()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "open", "quick/unverified must not be have_bytes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verified_offline_blob_is_have_bytes_not_missing() {
        let c = mem();
        setup_monitor(&c);
        let dir = std::env::temp_dir().join(format!("janus-radar-off-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let root = store::root_add(&c, "drawer", dir.to_str().unwrap(), "removable").unwrap();
        store::root_set_cold(&c, root, true).unwrap();
        c.execute("UPDATE storage_roots SET present=0 WHERE id=?1", [root]).unwrap();
        let sha = "44".repeat(32);
        let blob = store::blob_upsert(&c, "blake-off", Some(&sha), 1000, None).unwrap();
        store::file_upsert(&c, root, "Qwen-Q4_K_M.gguf", 1000, 0, 0, 0, 2, true, None, Some(blob), "full", "ok", None)
            .unwrap();
        sweep_ok(
            &c,
            vec![remote("main", "bartowski-Qwen-Q4_K_M.gguf", "bartowski", Some(sha.clone()))],
        );
        let rows = wanted(&c, &WantedFilter { have_offline: true, ..WantedFilter::default() }).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "skipped_have_bytes");
        assert!(!rows[0].local_present);
        assert!(rows[0].note.contains("offline"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
