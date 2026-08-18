use crate::store;
use rusqlite::Connection;

pub const PARAMS_SIM_RATIO: f64 = 0.15;
const STOP: &[&str] = &[
    "q2", "q3", "q4", "q5", "q6", "q8", "iq1", "iq2", "iq3", "iq4", "k", "s", "m", "l", "xl",
    "xxs", "xs", "f16", "f32", "bf16", "instruct", "chat", "base", "thinking", "unknown",
];

struct Fam {
    key: String,
    arch: Option<String>,
    total: Option<f64>,
    #[allow(dead_code)]
    active: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub a_key: String,
    pub b_key: String,
    pub reason: &'static str,
    pub shared_tokens: usize,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub code: String,
    pub count: i64,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub suggestions: Vec<Suggestion>,
}

pub fn report(conn: &Connection) -> Report {
    let mut findings = Vec::new();
    push_count(
        conn,
        &mut findings,
        "SELECT COUNT(*) FROM storage_roots WHERE present=0",
        "root.not_found",
        "offline or missing roots",
    );
    push_count(
        conn,
        &mut findings,
        "SELECT COUNT(*) FROM storage_roots WHERE mount_id IS NULL",
        "root.no_mount_id",
        "roots without a volume UUID/serial",
    );
    push_count(
        conn,
        &mut findings,
        "SELECT COUNT(*) FROM files WHERE hash_state != 'full'",
        "hash.unverified",
        "files not full-hashed",
    );
    push_count(
        conn,
        &mut findings,
        "SELECT COUNT(*) FROM files WHERE parse_error='placeholder'",
        "scan.placeholder",
        "skipped unhydrated cloud placeholders",
    );
    let fetch_n: i64 = conn
        .query_row("SELECT COUNT(*) FROM storage_roots WHERE kind='fetch'", [], |r| r.get(0))
        .unwrap_or(0);
    if fetch_n == 0 {
        findings.push(Finding {
            code: "root.not_writable".into(),
            count: 1,
            message: "no fetch root registered".into(),
        });
    }
    Report {
        findings,
        suggestions: sweep(conn),
    }
}

fn push_count(conn: &Connection, out: &mut Vec<Finding>, sql: &str, code: &str, message: &str) {
    let n: i64 = conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0);
    if n > 0 {
        out.push(Finding {
            code: code.to_string(),
            count: n,
            message: message.to_string(),
        });
    }
}

pub fn sweep(conn: &Connection) -> Vec<Suggestion> {
    let mut fams = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, family_key, arch, params_total, params_active FROM model_families ORDER BY id",
    ) {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok(Fam {
                key: r.get(1)?,
                arch: r.get(2)?,
                total: r.get(3)?,
                active: r.get(4)?,
            })
        }) {
            for r in rows.flatten() {
                fams.push(r);
            }
        }
    }
    let mut out = Vec::new();
    for i in 0..fams.len() {
        for j in (i + 1)..fams.len() {
            if fams[i].key == fams[j].key {
                continue;
            }
            if store::is_declined(conn, &fams[i].key, &fams[j].key, crate::FAMILY_KEY_ALGO)
                || store::is_aliased(conn, &fams[i].key, &fams[j].key)
            {
                continue;
            }
            let (shared, score) = name_similarity(&fams[i].key, &fams[j].key);
            if shared == 0 {
                continue;
            }
            let arch_eq = fams[i].arch == fams[j].arch;
            let total_sim = match (fams[i].total, fams[j].total) {
                (Some(a), Some(b)) => (a - b).abs() / a.max(b) <= PARAMS_SIM_RATIO,
                _ => false,
            };
            if !arch_eq {
                continue;
            }
            let reason = if total_sim { "params" } else { "name" };
            out.push(Suggestion {
                a_key: fams[i].key.clone(),
                b_key: fams[j].key.clone(),
                reason,
                shared_tokens: shared,
                score,
            });
        }
    }
    out
}

fn name_tokens(key: &str) -> Vec<String> {
    let name = key.split('|').next().unwrap_or(key);
    let size_re = regex::Regex::new(r"^\d+\.?\d*b$").unwrap();
    name.split('-')
        .map(|t| t.to_lowercase())
        .filter(|t| !t.is_empty() && !STOP.contains(&t.as_str()) && !size_re.is_match(t))
        .collect()
}

fn name_similarity(a: &str, b: &str) -> (usize, f64) {
    let ta = name_tokens(a);
    let tb = name_tokens(b);
    if ta.is_empty() || tb.is_empty() {
        return (0, 0.0);
    }
    let shared = ta.iter().filter(|t| tb.contains(t)).count();
    let union = {
        let mut s: Vec<String> = ta.clone();
        s.extend(tb.iter().filter(|t| !ta.contains(t)).cloned());
        s.len()
    };
    (shared, shared as f64 / union as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_emits_stable_codes() {
        use crate::{db, store};
        let c = db::open(None).unwrap();
        db::init_schema(&c).unwrap();
        let dir = std::env::temp_dir().join(format!("janus-doc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let id = store::root_add(&c, "models", dir.to_str().unwrap(), "internal").unwrap();
        c.execute(
            "INSERT INTO files (root_id, rel_path, size, mtime, ctime, dev, ino, is_symlink, hash_state, parse_state, state)
             VALUES (?1,'a.gguf',1,0,0,0,1,0,'none','ok','present')",
            [id],
        )
        .unwrap();
        let r = report(&c);
        assert!(r.findings.iter().any(|f| f.code == "hash.unverified"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moe_vs_dense_similar_but_distinct() {
        let shared = name_similarity("qwen3-30b-a3b|qwen3|t30.5|a3.0", "qwen3-32b|qwen3|t32|a32").0;
        assert!(shared >= 1, "share a meaningful token");
        let s2 = name_similarity("foo-8b-instruct|qwen3|t8.0|a8.0", "bar-8b-instruct|qwen3|t8.0|a8.0").0;
        assert_eq!(s2, 0, "foo vs bar share nothing meaningful");
    }
}
