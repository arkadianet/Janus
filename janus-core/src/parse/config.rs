use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct ConfigJson {
    pub name: Option<String>,
    pub arch: Option<String>,
    pub model_type: Option<String>,
}

pub fn read_file(path: &Path) -> Option<ConfigJson> {
    let raw = std::fs::read(path).ok()?;
    parse_bytes(&raw)
}

pub fn read_adjacent(weight_path: &Path) -> Option<ConfigJson> {
    let cfg = weight_path.parent()?.join("config.json");
    if !cfg.is_file() {
        return None;
    }
    read_file(&cfg)
}

pub fn parse_bytes(raw: &[u8]) -> Option<ConfigJson> {
    let v: Value = serde_json::from_slice(raw).ok()?;
    let obj = v.as_object()?;
    let model_type = obj.get("model_type").and_then(|x| x.as_str()).map(|s| s.to_string());
    let arch = obj
        .get("architectures")
        .and_then(|x| x.as_array())
        .and_then(|a| a.first())
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| model_type.clone());
    let name = obj
        .get("_name_or_path")
        .and_then(|x| x.as_str())
        .map(|s| s.trim_end_matches('/').rsplit('/').next().unwrap_or(s).to_string())
        .filter(|s| !s.is_empty());
    Some(ConfigJson { name, arch, model_type })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_json_name_from_path_field() {
        let raw = br#"{"architectures":["LlamaForCausalLM"],"model_type":"llama","_name_or_path":"meta-llama/Llama-3-8B"}"#;
        let c = parse_bytes(raw).unwrap();
        assert_eq!(c.arch.as_deref(), Some("LlamaForCausalLM"));
        assert_eq!(c.model_type.as_deref(), Some("llama"));
        assert_eq!(c.name.as_deref(), Some("Llama-3-8B"));
    }
}
