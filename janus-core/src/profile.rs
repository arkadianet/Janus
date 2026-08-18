use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::store;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QualityProfile {
    pub name: String,
    #[serde(default)]
    pub formats: Vec<String>,
    #[serde(default)]
    pub quants: Vec<String>,
    pub cutoff: Option<String>,
    #[serde(default)]
    pub publishers: Vec<String>,
    pub max_bytes: Option<u64>,
    #[serde(default)]
    pub exclude_name: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileRow {
    pub id: i64,
    #[serde(flatten)]
    pub spec: QualityProfile,
}

pub fn default_daily_llm() -> QualityProfile {
    QualityProfile {
        name: "daily-llm".into(),
        formats: vec!["gguf".into()],
        quants: vec!["Q4_K_M".into(), "Q5_K_M".into()],
        cutoff: Some("Q4_K_M".into()),
        publishers: vec!["bartowski".into(), "official".into()],
        max_bytes: Some(40 * 1024 * 1024 * 1024),
        exclude_name: vec!["i1".into(), "-IQ".into()],
    }
}

pub fn upsert(conn: &Connection, spec: &QualityProfile) -> Result<i64, String> {
    if spec.name.trim().is_empty() {
        return Err("identity.not_found".into());
    }
    let json = serde_json::to_string(spec).map_err(|e| format!("scan.io: {e}"))?;
    if let Some(id) = find_id(conn, &spec.name) {
        conn.execute("UPDATE quality_profiles SET spec_json=?1 WHERE id=?2", params![json, id])
            .map_err(store::to_err)?;
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO quality_profiles (name, spec_json) VALUES (?1, ?2)",
        params![spec.name, json],
    )
    .map_err(store::to_err)?;
    Ok(conn.last_insert_rowid())
}

pub fn find_id(conn: &Connection, name: &str) -> Option<i64> {
    conn.query_row("SELECT id FROM quality_profiles WHERE name=?1", [name], |r| r.get(0))
        .ok()
}

pub fn get(conn: &Connection, id: i64) -> Result<ProfileRow, String> {
    let (name, json): (String, String) = conn
        .query_row(
            "SELECT name, COALESCE(spec_json,'{}') FROM quality_profiles WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "identity.not_found".to_string())?;
    Ok(ProfileRow { id, spec: parse_spec(&name, &json) })
}

pub fn get_by_name(conn: &Connection, name: &str) -> Result<ProfileRow, String> {
    let id = find_id(conn, name).ok_or_else(|| "identity.not_found".to_string())?;
    get(conn, id)
}

pub fn list(conn: &Connection) -> Result<Vec<ProfileRow>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, COALESCE(spec_json,'{}') FROM quality_profiles ORDER BY name")
        .map_err(store::to_err)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
        .map_err(store::to_err)?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, json) = row.map_err(store::to_err)?;
        out.push(ProfileRow { id, spec: parse_spec(&name, &json) });
    }
    Ok(out)
}

pub fn ensure_default(conn: &Connection) -> Result<i64, String> {
    if let Some(id) = find_id(conn, "daily-llm") {
        return Ok(id);
    }
    upsert(conn, &default_daily_llm())
}

pub fn parse_bytes(s: &str) -> Option<u64> {
    let s = s.trim().replace('_', "");
    if s.is_empty() {
        return None;
    }
    let (num, mul) = if let Some(n) = s.strip_suffix("GiB").or_else(|| s.strip_suffix("gib")) {
        (n, 1024u64.pow(3))
    } else if let Some(n) = s.strip_suffix("MiB").or_else(|| s.strip_suffix("mib")) {
        (n, 1024u64.pow(2))
    } else if let Some(n) = s.strip_suffix("GB").or_else(|| s.strip_suffix("gb")) {
        (n, 1000u64.pow(3))
    } else if let Some(n) = s.strip_suffix("MB").or_else(|| s.strip_suffix("mb")) {
        (n, 1000u64.pow(2))
    } else if let Some(n) = s.strip_suffix('G').or_else(|| s.strip_suffix('g')) {
        (n, 1024u64.pow(3))
    } else if let Some(n) = s.strip_suffix('M').or_else(|| s.strip_suffix('m')) {
        (n, 1024u64.pow(2))
    } else {
        (s.as_str(), 1)
    };
    num.trim().parse::<f64>().ok().map(|v| (v * mul as f64) as u64)
}

pub fn quant_rank(q: &str) -> i32 {
    match q.to_ascii_uppercase().as_str() {
        "F32" => 110,
        "F16" => 100,
        "BF16" => 95,
        "Q8_K" => 88,
        "Q8_0" => 85,
        "Q6_K_XL" => 78,
        "Q6_K" => 75,
        "Q5_K_XL" | "Q5_K_L" => 68,
        "Q5_K_M" => 65,
        "Q5_K_S" => 62,
        "Q5_1" => 60,
        "Q5_0" => 58,
        "Q4_K_XL" | "Q4_K_L" => 55,
        "Q4_K_M" => 50,
        "Q4_K_S" => 47,
        "Q4_1" => 45,
        "Q4_0" => 42,
        "IQ4_NL" | "IQ4_XS" => 40,
        "Q3_K_XL" | "Q3_K_L" => 38,
        "Q3_K_M" => 35,
        "Q3_K_S" => 32,
        "IQ3_M" | "IQ3_S" | "IQ3_XS" | "IQ3_XXS" => 28,
        "Q2_K_S" | "Q2_K" => 25,
        "IQ2_M" | "IQ2_S" | "IQ2_XS" | "IQ2_XXS" => 20,
        "IQ1_M" | "IQ1_S" => 10,
        _ => 0,
    }
}

pub fn meets_cutoff(have: &str, cutoff: &str) -> bool {
    let hr = quant_rank(have);
    let cr = quant_rank(cutoff);
    hr > 0 && cr > 0 && hr >= cr
}

fn parse_spec(name: &str, json: &str) -> QualityProfile {
    let mut spec: QualityProfile = serde_json::from_str(json).unwrap_or_else(|_| QualityProfile {
        name: name.to_string(),
        formats: Vec::new(),
        quants: Vec::new(),
        cutoff: None,
        publishers: Vec::new(),
        max_bytes: None,
        exclude_name: Vec::new(),
    });
    if spec.name.is_empty() {
        spec.name = name.to_string();
    }
    spec
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn default_profile_roundtrip() {
        let c = db::open(None).unwrap();
        db::init_schema(&c).unwrap();
        let id = ensure_default(&c).unwrap();
        let row = get(&c, id).unwrap();
        assert_eq!(row.spec.name, "daily-llm");
        assert_eq!(row.spec.cutoff.as_deref(), Some("Q4_K_M"));
        assert_eq!(row.spec.publishers[0], "bartowski");
        assert_eq!(ensure_default(&c).unwrap(), id);
    }

    #[test]
    fn cutoff_q3_does_not_meet_q4() {
        assert!(!meets_cutoff("Q3_K_M", "Q4_K_M"));
        assert!(meets_cutoff("Q4_K_M", "Q4_K_M"));
        assert!(meets_cutoff("Q8_0", "Q4_K_M"));
    }
}
