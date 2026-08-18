use crate::ev::{Kind, Role};
use std::path::Path;
use std::sync::LazyLock;

const QUANT_TAGS: &[&str] = &[
    "Q8_K",
    "Q6_K_XL",
    "Q5_K_XL",
    "Q4_K_XL",
    "Q3_K_XL",
    "Q5_K_M",
    "Q4_K_M",
    "Q5_K_S",
    "Q4_K_S",
    "Q4_K_L",
    "Q5_K_L",
    "Q3_K_M",
    "Q3_K_S",
    "Q3_K_L",
    "Q2_K_S",
    "Q2_K",
    "Q6_K",
    "IQ3_XXS",
    "IQ3_XS",
    "IQ2_XXS",
    "IQ4_NL",
    "IQ4_XS",
    "IQ1_M",
    "IQ1_S",
    "IQ2_M",
    "IQ2_S",
    "IQ2_XS",
    "IQ3_M",
    "IQ3_S",
    "Q8_0",
    "Q5_1",
    "Q5_0",
    "Q4_1",
    "Q4_0",
    "BF16",
    "F16",
    "F32",
];

pub const PUBLISHERS: &[&str] = &[
    "bartowski",
    "mradermacher",
    "TheBloke",
    "turboderp",
    "Orenguteng",
];

pub fn stem(file: &str) -> String {
    let name = Path::new(file).file_name().and_then(|n| n.to_str()).unwrap_or(file);
    let name = name.strip_suffix(".gguf").unwrap_or(name);
    let name = name.strip_suffix(".safetensors").unwrap_or(name);
    name.to_string()
}

pub fn is_partial(file: &str) -> bool {
    let lower = file.replace('\\', "/").to_lowercase();
    if lower.contains("/.janus-partial/") {
        return true;
    }
    let name = Path::new(&lower).file_name().and_then(|n| n.to_str()).unwrap_or(&lower);
    name.ends_with(".part")
        || name.ends_with(".part_file")
        || name.ends_with(".aria2")
        || name.ends_with(".!qb")
        || name.ends_with(".crdownload")
}

pub fn is_model_index(file: &str) -> bool {
    Path::new(file)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case("model_index.json"))
        .unwrap_or(false)
}

pub fn is_config_json(file: &str) -> bool {
    Path::new(file)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case("config.json"))
        .unwrap_or(false)
}

pub fn is_weight_index(file: &str) -> bool {
    file.replace('\\', "/").to_lowercase().ends_with(".index.json") && !is_model_index(file)
}

pub fn hf_cache_repo(rel: &str) -> Option<(String, String)> {
    let norm = rel.replace('\\', "/");
    let re = regex::Regex::new(r"models--([^/]+)/snapshots/([^/]+)").ok()?;
    let caps = re.captures(&norm)?;
    let repo = caps.get(1)?.as_str().replace("--", "/");
    let rev = caps.get(2)?.as_str().to_string();
    if repo.is_empty() || rev.is_empty() {
        None
    } else {
        Some((repo, rev))
    }
}

pub fn hf_snapshot_name(rel: &str) -> Option<String> {
    let (repo, _) = hf_cache_repo(rel)?;
    repo.rsplit('/').next().map(|s| s.to_string()).filter(|s| !s.is_empty())
}

pub fn role_from_name(file: &str) -> Role {
    if is_model_index(file) || is_config_json(file) || is_weight_index(file) {
        return Role::Config;
    }
    let s = stem(file).to_lowercase();
    if s.starts_with("mmproj") || s.contains("mmproj") || s.contains("vision_projector") {
        return Role::Mmproj;
    }
    if s.contains("tokenizer") || s.contains("vocab") {
        return Role::Tokenizer;
    }
    if s == "config" || s == "config.json" || s == "model_index.json" {
        return Role::Config;
    }
    if s.ends_with(".lora") || s.contains("-lora") || s.contains("-loras") {
        return Role::Lora;
    }
    if is_shard(file) {
        return Role::Shard;
    }
    if s.contains("fastmtp") || s.contains("-mtp-") || s.contains("-draft") || s.contains("speculative") {
        return Role::Sidecar;
    }
    Role::Weights
}

pub fn is_shard(file: &str) -> bool {
    shard_strip(file).is_some()
}

static SHARD_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"-\d{1,5}-of-\d{1,5}(?:-[^/]*)?$").expect("shard regex"));
static DISPLAY_STEM_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)(^|[^a-z0-9])(?:ud|a\d+b)($|[^a-z0-9])").expect("display stem regex"));

pub fn shard_strip(file: &str) -> Option<String> {
    let s = stem(file);
    if SHARD_RE.is_match(&s) {
        Some(SHARD_RE.replace_all(&s, "").to_string())
    } else {
        None
    }
}

pub fn quant_tag(stem: &str) -> Option<String> {
    let upper = stem.to_uppercase();
    for t in QUANT_TAGS {
        if upper.contains(t) {
            return Some((*t).to_string());
        }
    }
    None
}

pub fn publisher_token(stem: &str) -> Option<String> {
    for p in PUBLISHERS {
        if stem.to_lowercase().contains(&p.to_lowercase()) {
            return Some((*p).to_string());
        }
    }
    None
}

pub fn params_tag_full(stem: &str) -> Option<f64> {
    let re = regex::Regex::new(r"(?i)(\d+(?:\.\d+)?)\s*b\b").unwrap();
    re.captures(stem)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<f64>().ok())
}

pub fn subflavour_tag(stem: &str) -> Option<&'static str> {
    let s = stem.to_lowercase();
    for t in &["instruct", "chat", "base", "thinking", "coder", "reasoning", "reasoner"] {
        if s.contains(t) {
            return Some(t);
        }
    }
    None
}

pub fn kind_from_name(stem: &str) -> Option<Kind> {
    let s = stem.to_lowercase();
    if s.contains("reranker") || s.contains("rerank") {
        return Some(Kind::Rerank);
    }
    if s.contains("nomic-embed") || s.contains("-embed") || s.contains("text-embedding") {
        return Some(Kind::Embeddings);
    }
    if s.contains("whisper") || s.contains("stt") || s.contains("-tts") {
        return Some(Kind::Audio);
    }
    None
}

pub fn display_stem(file: &str) -> String {
    let mut s = shard_strip(file).unwrap_or_else(|| stem(file));
    s = s.to_lowercase();
    if let Some(q) = quant_tag(&s) {
        s = s.replace(&q.to_lowercase(), "");
    }
    for p in PUBLISHERS {
        s = s.replace(&p.to_lowercase(), "");
    }
    s = DISPLAY_STEM_RE.replace_all(&s, "-").to_string();
    slug(&s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_suffixes() {
        assert!(is_partial("model.gguf.crdownload"));
        assert!(is_partial("other.gguf.part"));
        assert!(is_partial("fetch/.janus-partial/12.part"));
        assert!(!is_partial("model.gguf"));
    }

    #[test]
    fn hf_cache_repo_from_snapshot_path() {
        let (repo, rev) = hf_cache_repo("hub/models--Qwen--Qwen3-8B/snapshots/abc123def/model.safetensors").unwrap();
        assert_eq!(repo, "Qwen/Qwen3-8B");
        assert_eq!(rev, "abc123def");
        assert_eq!(hf_snapshot_name("hub/models--Qwen--Qwen3-8B/snapshots/abc123/x.safetensors").as_deref(), Some("Qwen3-8B"));
    }
}

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}