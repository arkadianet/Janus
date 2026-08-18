//! Shared catalogue query engine. CLI `--json` and the HTTP API use these
//! payloads so list/search/show/storage never fork.

use rusqlite::Connection;
use serde::Serialize;

use crate::{dedup, search, store};

#[derive(Debug, Clone, Default)]
pub struct ModelFilter {
    pub kind: Option<String>,
    pub family: Option<String>,
    pub root: Option<String>,
    pub offline: bool,
    pub dups: bool,
    pub q: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct FileFilter {
    pub root: Option<i64>,
    pub state: Option<String>,
    pub hash_state: Option<String>,
    pub unknown: bool,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Fact<T: Serialize> {
    pub value: T,
    pub level: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Home {
    pub counts: HomeCounts,
    pub roots: Vec<RootJson>,
    pub recent: Vec<FileJson>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HomeCounts {
    pub families: i64,
    pub families_inferred: i64,
    pub files: i64,
    pub bytes: i64,
    pub unverified: i64,
    pub unknown_files: i64,
    pub reclaimable: i64,
    pub roots: i64,
    pub roots_present: i64,
    pub wanted_open: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RootJson {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub path: String,
    pub present: bool,
    pub cold: bool,
    pub writable: bool,
    pub mount_id: Option<String>,
    pub last_present_check: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelList {
    pub families: Vec<ModelRow>,
    pub counts: ListCounts,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListCounts {
    pub families: usize,
    pub families_inferred: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelRow {
    pub id: i64,
    pub family_key: String,
    pub name: Fact<Option<String>>,
    pub kind: Fact<String>,
    pub params_total: Option<f64>,
    pub quants: String,
    pub bytes: i64,
    pub roots: Vec<ModelRoot>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelRoot {
    pub name: String,
    pub present: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelDetail {
    pub family: ModelRow,
    pub variants: Vec<VariantJson>,
    pub files: Vec<FileJson>,
    pub evidence: Vec<EvidenceJson>,
    pub provenance: Vec<ProvenanceJson>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VariantJson {
    pub quant: Fact<String>,
    pub format: Fact<String>,
    pub subflavour: Fact<String>,
    pub publisher: Fact<String>,
    pub bytes: i64,
    pub root: String,
    pub present: bool,
    pub last_file_mtime: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileJson {
    pub id: i64,
    pub root_id: i64,
    pub root: String,
    pub rel_path: String,
    pub size: i64,
    pub state: String,
    pub hash_state: String,
    pub parse_state: String,
    pub present: bool,
    pub unknown: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceJson {
    pub field: String,
    pub value: String,
    pub level: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProvenanceJson {
    pub event: String,
    pub source_kind: String,
    pub repo: Option<String>,
    pub revision: Option<String>,
    pub at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageJson {
    pub roots: Vec<store::StorageRow>,
    pub reclaimable: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchJson {
    pub hits: Vec<HitJson>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HitJson {
    pub kind: String,
    pub name: String,
    pub key: Option<String>,
    pub path: Option<String>,
    pub present: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DupsJson {
    pub groups: Vec<DupJson>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DupJson {
    pub blake3: String,
    pub size: i64,
    pub copies: usize,
    pub allocations: i64,
    pub reclaimable: i64,
    pub paths: Vec<String>,
}

pub fn home(conn: &Connection) -> Result<Home, String> {
    let (families, families_inferred, bytes, unverified, unknown_files) = store::home_counts(conn)?;
    let (roots_all, roots_present) = store::present_count(conn)?;
    let files: i64 = conn
        .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
        .map_err(store::to_err)?;
    let storage = store::storage_summary(conn)?;
    let reclaimable = storage.iter().filter(|r| r.present).map(|r| r.reclaimable).sum();
    Ok(Home {
        counts: HomeCounts {
            families,
            families_inferred,
            files,
            bytes,
            unverified,
            unknown_files,
            reclaimable,
            roots: roots_all,
            roots_present,
            wanted_open: crate::radar::wanted_open_count(conn),
        },
        roots: roots(conn)?,
        recent: files_list(conn, &FileFilter { limit: Some(12), ..FileFilter::default() })?,
    })
}

pub fn roots(conn: &Connection) -> Result<Vec<RootJson>, String> {
    Ok(store::root_ls(conn)?
        .into_iter()
        .map(|r| RootJson {
            id: r.id,
            name: r.name,
            kind: r.kind,
            path: r.path,
            present: r.present.unwrap_or(0) == 1,
            cold: r.cold == 1,
            writable: r.writable == 1,
            mount_id: r.mount_id,
            last_present_check: r.last_present_check,
        })
        .collect())
}

pub fn models(conn: &Connection, filter: &ModelFilter) -> Result<ModelList, String> {
    let mut fams = store::family_list(conn)?;
    if let Some(kind) = &filter.kind {
        fams.retain(|f| f.kind == *kind);
    }
    if let Some(family) = &filter.family {
        let needle = family.to_lowercase();
        fams.retain(|f| {
            f.key.to_lowercase().contains(&needle)
                || f.name.as_deref().unwrap_or("").to_lowercase().contains(&needle)
        });
    }
    if let Some(root) = &filter.root {
        fams.retain(|f| f.roots.iter().any(|(n, _)| n == root));
    }
    if filter.offline {
        fams.retain(|f| f.roots.iter().any(|(_, present)| !*present));
    }
    if let Some(q) = &filter.q {
        let needle = q.to_lowercase();
        fams.retain(|f| {
            f.key.to_lowercase().contains(&needle)
                || f.name.as_deref().unwrap_or("").to_lowercase().contains(&needle)
                || f.quants.to_lowercase().contains(&needle)
        });
    }
    if filter.dups {
        let plan = dedup::plan(conn);
        let dup_families: std::collections::HashSet<i64> = plan
            .iter()
            .flat_map(|g| g.copies.iter())
            .filter_map(|c| {
                conn.query_row(
                    "SELECT COALESCE(v.family_id, fr.family_id) FROM files f
                     JOIN file_roles fr ON fr.file_id=f.id
                     LEFT JOIN model_variants v ON v.id=fr.variant_id
                     WHERE f.root_id=?1 AND f.rel_path=?2",
                    rusqlite::params![c.root_id, c.rel_path],
                    |r| r.get::<_, Option<i64>>(0),
                )
                .ok()
                .flatten()
            })
            .collect();
        fams.retain(|f| dup_families.contains(&f.id));
    }
    if let Some(limit) = filter.limit {
        fams.truncate(limit);
    }
    let inferred = fams.iter().filter(|f| f.name_level.as_deref() == Some("inferred")).count();
    let families: Vec<ModelRow> = fams.iter().map(model_row).collect();
    let n = families.len();
    Ok(ModelList {
        families,
        counts: ListCounts { families: n, families_inferred: inferred },
    })
}

pub fn model(conn: &Connection, id: i64) -> Result<ModelDetail, String> {
    let fams = store::family_list(conn)?;
    let f = fams.into_iter().find(|x| x.id == id).ok_or_else(|| "identity.not_found".to_string())?;
    let variants = store::family_variants(conn, id)?
        .into_iter()
        .map(|v| {
            let q_level = evidence_level(conn, "variant", id, "quant").unwrap_or_else(|| {
                if v.quant == "unknown" {
                    "inferred".into()
                } else {
                    "detected".into()
                }
            });
            VariantJson {
                quant: Fact { value: v.quant, level: q_level },
                format: Fact { value: v.format, level: "detected".into() },
                subflavour: Fact { value: v.subflavour, level: "inferred".into() },
                publisher: Fact { value: v.publisher, level: "inferred".into() },
                bytes: v.bytes,
                root: v.root,
                present: v.present,
                last_file_mtime: v.last_file_mtime,
            }
        })
        .collect();
    Ok(ModelDetail {
        family: model_row(&f),
        variants,
        files: family_files(conn, id)?,
        evidence: family_evidence(conn, id)?,
        provenance: family_provenance(conn, id)?,
    })
}

pub fn files_list(conn: &Connection, filter: &FileFilter) -> Result<Vec<FileJson>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT f.id, f.root_id, r.name, f.rel_path, COALESCE(f.size,0), f.state,
                    COALESCE(f.hash_state,'none'), COALESCE(f.parse_state,'pending'),
                    COALESCE(r.present,0),
                    CASE WHEN fr.file_id IS NULL THEN 1 ELSE 0 END
               FROM files f
               JOIN storage_roots r ON r.id=f.root_id
               LEFT JOIN file_roles fr ON fr.file_id=f.id
              ORDER BY f.mtime DESC, f.id DESC",
        )
        .map_err(store::to_err)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(FileJson {
                id: r.get(0)?,
                root_id: r.get(1)?,
                root: r.get(2)?,
                rel_path: r.get(3)?,
                size: r.get(4)?,
                state: r.get(5)?,
                hash_state: r.get(6)?,
                parse_state: r.get(7)?,
                present: r.get::<_, i64>(8)? == 1,
                unknown: r.get::<_, i64>(9)? == 1,
            })
        })
        .map_err(store::to_err)?;
    let mut out = rows.collect::<Result<Vec<_>, _>>().map_err(store::to_err)?;
    if let Some(root) = filter.root {
        out.retain(|f| f.root_id == root);
    }
    if let Some(state) = &filter.state {
        out.retain(|f| f.state == *state);
    }
    if let Some(hash_state) = &filter.hash_state {
        out.retain(|f| f.hash_state == *hash_state);
    }
    if filter.unknown {
        out.retain(|f| f.unknown);
    }
    if let Some(limit) = filter.limit {
        out.truncate(limit);
    }
    Ok(out)
}

pub fn file(conn: &Connection, id: i64) -> Result<FileJson, String> {
    files_list(conn, &FileFilter::default())?
        .into_iter()
        .find(|f| f.id == id)
        .ok_or_else(|| "identity.not_found".to_string())
}

pub fn search_json(conn: &Connection, q: &str) -> Result<SearchJson, String> {
    let hits = search::search(conn, q)?
        .into_iter()
        .map(|h| HitJson {
            kind: h.kind.to_string(),
            name: h.name,
            key: h.key,
            path: h.path,
            present: h.present,
        })
        .collect();
    Ok(SearchJson { hits })
}

pub fn storage(conn: &Connection) -> Result<StorageJson, String> {
    let roots = store::storage_summary(conn)?;
    let reclaimable = roots.iter().filter(|r| r.present).map(|r| r.reclaimable).sum();
    Ok(StorageJson { roots, reclaimable })
}

#[derive(Debug, Clone, Serialize)]
pub struct WantedJson {
    pub items: Vec<crate::radar::WantedRow>,
    pub privacy_notice: String,
}

pub fn wanted(conn: &Connection, filter: &crate::radar::WantedFilter) -> Result<WantedJson, String> {
    Ok(WantedJson {
        items: crate::radar::wanted(conn, filter)?,
        privacy_notice: crate::radar::PRIVACY_NOTICE.into(),
    })
}

pub fn profiles(conn: &Connection) -> Result<Vec<crate::profile::ProfileRow>, String> {
    crate::profile::ensure_default(conn)?;
    crate::profile::list(conn)
}

pub fn monitors(conn: &Connection) -> Result<Vec<crate::radar::MonitorRow>, String> {
    crate::radar::monitor_list(conn)
}

pub fn dups(conn: &Connection) -> Result<DupsJson, String> {
    let groups = dedup::plan(conn)
        .into_iter()
        .map(|g| DupJson {
            blake3: g.blake3,
            size: g.size,
            copies: g.copies.len(),
            allocations: g.allocations,
            reclaimable: g.reclaimable_bytes,
            paths: g.copies.iter().map(|c| format!("{}/{}", c.root_name, c.rel_path)).collect(),
        })
        .collect();
    Ok(DupsJson { groups })
}

fn model_row(f: &store::ListFamily) -> ModelRow {
    let level = f.name_level.clone().unwrap_or_else(|| "inferred".into());
    ModelRow {
        id: f.id,
        family_key: f.key.clone(),
        name: Fact { value: f.name.clone(), level },
        kind: Fact { value: f.kind.clone(), level: "detected".into() },
        params_total: f.params_total,
        quants: f.quants.clone(),
        bytes: f.bytes,
        roots: f.roots.iter().map(|(n, p)| ModelRoot { name: n.clone(), present: *p }).collect(),
    }
}

fn evidence_level(conn: &Connection, subject_type: &str, subject_id: i64, field: &str) -> Option<String> {
    conn.query_row(
        "SELECT level FROM evidence WHERE subject_type=?1 AND subject_id=?2 AND field=?3 ORDER BY id DESC LIMIT 1",
        rusqlite::params![subject_type, subject_id, field],
        |r| r.get(0),
    )
    .ok()
}

fn family_files(conn: &Connection, family_id: i64) -> Result<Vec<FileJson>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT f.id, f.root_id, r.name, f.rel_path, COALESCE(f.size,0), f.state,
                    COALESCE(f.hash_state,'none'), COALESCE(f.parse_state,'pending'),
                    COALESCE(r.present,0), 0
               FROM files f
               JOIN storage_roots r ON r.id=f.root_id
               JOIN file_roles fr ON fr.file_id=f.id
               LEFT JOIN model_variants v ON v.id=fr.variant_id
              WHERE v.family_id=?1 OR fr.family_id=?1
              ORDER BY f.rel_path",
        )
        .map_err(store::to_err)?;
    let rows = stmt
        .query_map([family_id], |r| {
            Ok(FileJson {
                id: r.get(0)?,
                root_id: r.get(1)?,
                root: r.get(2)?,
                rel_path: r.get(3)?,
                size: r.get(4)?,
                state: r.get(5)?,
                hash_state: r.get(6)?,
                parse_state: r.get(7)?,
                present: r.get::<_, i64>(8)? == 1,
                unknown: false,
            })
        })
        .map_err(store::to_err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(store::to_err)
}

fn family_evidence(conn: &Connection, family_id: i64) -> Result<Vec<EvidenceJson>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT field, value, level, COALESCE(source,'') FROM evidence
              WHERE subject_type='family' AND subject_id=?1 ORDER BY id",
        )
        .map_err(store::to_err)?;
    let rows = stmt
        .query_map([family_id], |r| {
            Ok(EvidenceJson {
                field: r.get(0)?,
                value: r.get(1)?,
                level: r.get(2)?,
                source: r.get(3)?,
            })
        })
        .map_err(store::to_err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(store::to_err)
}

fn family_provenance(conn: &Connection, family_id: i64) -> Result<Vec<ProvenanceJson>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT event, COALESCE(source_kind,''), repo, revision, at FROM provenance_entries
              WHERE subject_type='family' AND subject_id=?1 ORDER BY id",
        )
        .map_err(store::to_err)?;
    let rows = stmt
        .query_map([family_id], |r| {
            Ok(ProvenanceJson {
                event: r.get(0)?,
                source_kind: r.get(1)?,
                repo: r.get(2)?,
                revision: r.get(3)?,
                at: r.get(4)?,
            })
        })
        .map_err(store::to_err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(store::to_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn mem() -> Connection {
        let c = db::open(None).unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    #[test]
    fn home_splits_inferred_from_known() {
        let c = mem();
        let known = store::family_insert(&c, "qwen|qwen|t7|a7", Some("Qwen"), Some("qwen"), Some(7.0), None, None, "llm").unwrap();
        store::evidence_put(&c, "family", known, "name", "Qwen", "known", "header");
        store::family_insert(&c, "mystery|unk|t0|a0", Some("mystery-file"), None, None, None, None, "unknown").unwrap();
        let h = home(&c).unwrap();
        assert_eq!(h.counts.families, 2);
        assert_eq!(h.counts.families_inferred, 1);
        assert_eq!(h.counts.roots, 0);
    }

    #[test]
    fn list_wraps_name_with_truth_level() {
        let c = mem();
        let id = store::family_insert(&c, "foo|llama|t8|a8", Some("Foo"), Some("llama"), Some(8.0), None, None, "llm").unwrap();
        store::evidence_put(&c, "family", id, "name", "Foo", "inferred", "filename");
        let list = models(&c, &ModelFilter::default()).unwrap();
        assert_eq!(list.counts.families, 1);
        assert_eq!(list.counts.families_inferred, 1);
        assert_eq!(list.families[0].name.value.as_deref(), Some("Foo"));
        assert_eq!(list.families[0].name.level, "inferred");
        assert_eq!(list.families[0].kind.value, "llm");
    }

    #[test]
    fn search_and_storage_share_engine() {
        let c = mem();
        store::family_insert(&c, "qwen3-coder|qwen3|t30|a30", Some("Qwen3-Coder"), None, None, None, None, "llm").unwrap();
        let hits = search_json(&c, "qwen").unwrap();
        assert_eq!(hits.hits.len(), 1);
        assert_eq!(hits.hits[0].kind, "family");
        let st = storage(&c).unwrap();
        assert!(st.roots.is_empty());
        assert_eq!(st.reclaimable, 0);
    }

    #[test]
    fn empty_home_has_zero_counts() {
        let c = mem();
        let h = home(&c).unwrap();
        assert_eq!(h.counts.families, 0);
        assert_eq!(h.counts.families_inferred, 0);
        assert_eq!(h.counts.wanted_open, 0);
        assert!(h.roots.is_empty());
        assert!(h.recent.is_empty());
    }
}
