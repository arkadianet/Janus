use crate::ev::{Kind, Role};
use std::path::Path;

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
    "Q2_K",
    "Q6_K",
    "IQ3_XXS",
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

pub fn role_from_name(file: &str) -> Role {
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

pub fn shard_strip(file: &str) -> Option<String> {
    let s = stem(file);
    let re = regex::Regex::new(r"-\d{1,5}-of-\d{1,5}(?:-[^/]*)?$").unwrap();
    if re.is_match(&s) {
        Some(re.replace_all(&s, "").to_string())
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
    if let Some(q) = quant_tag(&s) {
        s = s.replace(&q, "");
    }
    for p in PUBLISHERS {
        if s.to_lowercase().contains(&p.to_lowercase()) {
            s = s.replace(&p.to_lowercase(), "");
        }
    }
    let re = regex::Regex::new(r"(?i)(^|[^a-z0-9])(?:ud|a\d+b)($|[^a-z0-9])").unwrap();
    s = re.replace_all(&s, "-").to_string();
    slug(&s)
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