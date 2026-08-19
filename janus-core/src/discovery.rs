use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DiscoveryHint {
    pub name: String,
    pub path: PathBuf,
}

pub fn path_is_discovery(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/").to_lowercase();
    s.contains("/.ollama/")
        || s.ends_with("/.ollama/models")
        || s.contains("huggingface")
        || s.contains("lm-studio")
        || s.contains("lmstudio")
}

pub fn refuse_write(kind: &str) -> Option<&'static str> {
    if kind == "discovery" {
        Some("root.discovery_readonly")
    } else if kind != "fetch" {
        Some("root.not_writable")
    } else {
        None
    }
}

pub fn candidates() -> Vec<DiscoveryHint> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("OLLAMA_MODELS") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            out.push(DiscoveryHint { name: "ollama".into(), path: p });
        }
    } else if let Some(h) = dirs::home_dir() {
        let p = h.join(".ollama").join("models");
        if p.is_dir() {
            out.push(DiscoveryHint { name: "ollama".into(), path: p });
        }
    }

    let hf = std::env::var("HF_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".cache").join("huggingface")));
    if let Some(base) = hf {
        let p = if base.ends_with("hub") { base } else { base.join("hub") };
        if p.is_dir() {
            out.push(DiscoveryHint { name: "hf-cache".into(), path: p });
        }
    }

    if let Some(h) = dirs::home_dir() {
        for (name, p) in [
            ("lmstudio", h.join(".cache").join("lm-studio").join("models")),
            ("lmstudio", h.join("Documents").join("LM Studio").join("models")),
        ] {
            if p.is_dir() && !out.iter().any(|x| x.path == p) {
                out.push(DiscoveryHint { name: name.into(), path: p });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_path_is_discovery() {
        assert!(path_is_discovery(Path::new("/home/you/.ollama/models")));
        assert!(path_is_discovery(Path::new("C:\\Users\\you\\.cache\\huggingface\\hub")));
        assert!(!path_is_discovery(Path::new("/home/you/models")));
    }

    #[test]
    fn discovery_kind_refuses_write() {
        assert_eq!(refuse_write("discovery"), Some("root.discovery_readonly"));
        assert_eq!(refuse_write("internal"), Some("root.not_writable"));
        assert_eq!(refuse_write("fetch"), None);
    }
}
