use serde::{Deserialize, Serialize};

/// One remote file from an AvailabilityProvider listing. No bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteFile {
    pub repo: String,
    pub revision: String,
    pub filename: String,
    pub size: Option<i64>,
    pub sha256: Option<String>,
    pub publisher: String,
}

pub trait AvailabilityProvider {
    fn list(&self, repo: &str, revision: Option<&str>) -> Result<Vec<RemoteFile>, String>;
}

#[derive(Debug, Clone, Default)]
pub struct MemoryProvider {
    pub files: Vec<RemoteFile>,
}

impl AvailabilityProvider for MemoryProvider {
    fn list(&self, repo: &str, revision: Option<&str>) -> Result<Vec<RemoteFile>, String> {
        Ok(self
            .files
            .iter()
            .filter(|f| f.repo == repo && revision.map(|r| r == f.revision).unwrap_or(true))
            .cloned()
            .collect())
    }
}

pub fn infer_publisher(filename: &str, repo: &str) -> String {
    if let Some(p) = crate::filename::publisher_token(filename) {
        return p;
    }
    let owner = repo.split('/').next().unwrap_or("");
    for p in crate::filename::PUBLISHERS {
        if owner.eq_ignore_ascii_case(p) {
            return (*p).to_string();
        }
    }
    if owner.is_empty() {
        "unknown".into()
    } else {
        "official".into()
    }
}

pub fn format_of(filename: &str) -> String {
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".gguf") {
        "gguf".into()
    } else if lower.ends_with(".safetensors") {
        "safetensors".into()
    } else if lower.ends_with(".onnx") {
        "onnx".into()
    } else {
        "unknown".into()
    }
}

pub fn remote_key(provider: &str, repo: &str, revision: &str, filename: &str) -> String {
    format!("{provider}|{repo}|{revision}|{filename}")
}

/// Cached HF tree listings. Network only when the caller opted in.
pub struct HfListingCache {
    pub cache_dir: std::path::PathBuf,
    pub allow_network: bool,
    pub token: Option<String>,
    client: Box<dyn HfHttp>,
}

pub trait HfHttp: Send + Sync {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HfResponse, String>;
}

pub struct HfResponse {
    pub status: u16,
    pub etag: Option<String>,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedListing {
    etag: Option<String>,
    fetched_at: i64,
    files: Vec<RemoteFile>,
}

impl AvailabilityProvider for HfListingCache {
    fn list(&self, repo: &str, revision: Option<&str>) -> Result<Vec<RemoteFile>, String> {
        self.fetch(repo, revision.unwrap_or("main"))
    }
}

pub fn live_hf(allow_network: bool) -> HfListingCache {
    let token = if allow_network {
        std::env::var("HF_TOKEN").ok().filter(|s| !s.is_empty())
    } else {
        None
    };
    HfListingCache {
        cache_dir: crate::store::cache_dir().join("http").join("hf"),
        allow_network,
        token,
        client: Box::new(UreqClient),
    }
}

struct UreqClient;

impl HfHttp for UreqClient {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HfResponse, String> {
        let mut req = ureq::get(url);
        for (k, v) in headers {
            req = req.set(k, v);
        }
        match req.call() {
            Ok(resp) => {
                let status = resp.status();
                let etag = resp.header("etag").map(|s| s.to_string());
                let body = resp.into_string().map_err(|e| format!("scan.io: {e}"))?;
                Ok(HfResponse { status, etag, body })
            }
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                Ok(HfResponse { status: code, etag: None, body })
            }
            Err(e) => Err(format!("network.disabled: {e}")),
        }
    }
}

impl HfListingCache {
    pub fn memory(allow_network: bool) -> Self {
        Self {
            cache_dir: crate::store::cache_dir().join("http").join("hf"),
            allow_network,
            token: None,
            client: Box::new(BlockedHttp),
        }
    }

    pub fn with_client(cache_dir: std::path::PathBuf, allow_network: bool, client: Box<dyn HfHttp>) -> Self {
        Self { cache_dir, allow_network, token: None, client }
    }

    pub fn list_cached(&self, repo: &str, revision: &str) -> Option<Vec<RemoteFile>> {
        let path = self.path_for(repo, revision);
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str::<CachedListing>(&raw).ok().map(|c| c.files)
    }

    pub fn store(&self, repo: &str, revision: &str, etag: Option<&str>, files: &[RemoteFile]) -> Result<(), String> {
        let path = self.path_for(repo, revision);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("scan.io: {e}"))?;
        }
        let payload = CachedListing {
            etag: etag.map(|s| s.to_string()),
            fetched_at: now_secs(),
            files: files.to_vec(),
        };
        std::fs::write(path, serde_json::to_string(&payload).map_err(|e| format!("scan.io: {e}"))?)
            .map_err(|e| format!("scan.io: {e}"))
    }

    pub fn fetch(&self, repo: &str, revision: &str) -> Result<Vec<RemoteFile>, String> {
        if let Some(hit) = self.list_cached(repo, revision) {
            if !self.allow_network {
                return Ok(hit);
            }
        } else if !self.allow_network {
            return Err("network.disabled".into());
        }
        if !self.allow_network {
            return self.list_cached(repo, revision).ok_or_else(|| "network.disabled".to_string());
        }
        let url = format!(
            "https://huggingface.co/api/models/{}/tree/{}?recursive=1",
            repo,
            revision
        );
        let auth = self.token.as_ref().map(|t| format!("Bearer {t}"));
        let mut headers: Vec<(&str, &str)> = Vec::new();
        if let Some(a) = auth.as_deref() {
            headers.push(("Authorization", a));
        }
        let resp = self.client.get(&url, &headers)?;
        if resp.status == 304 {
            return self.list_cached(repo, revision).ok_or_else(|| "network.disabled".to_string());
        }
        if resp.status >= 400 {
            return Err(format!("network.disabled: hf {status}", status = resp.status));
        }
        let files = parse_hf_tree(repo, revision, &resp.body)?;
        self.store(repo, revision, resp.etag.as_deref(), &files)?;
        Ok(files)
    }

    fn path_for(&self, repo: &str, revision: &str) -> std::path::PathBuf {
        let safe_repo = repo.replace('/', "--").replace('\\', "--");
        let safe_rev = revision.replace(['/', '\\', ':'], "_");
        self.cache_dir.join(safe_repo).join(format!("{safe_rev}.json"))
    }
}

struct BlockedHttp;

impl HfHttp for BlockedHttp {
    fn get(&self, _url: &str, _headers: &[(&str, &str)]) -> Result<HfResponse, String> {
        Err("network.disabled".into())
    }
}

pub fn parse_hf_tree(repo: &str, revision: &str, body: &str) -> Result<Vec<RemoteFile>, String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| format!("scan.io: {e}"))?;
    let arr = v.as_array().ok_or_else(|| "scan.io: hf tree not an array".to_string())?;
    let mut files = Vec::new();
    for item in arr {
        if item.get("type").and_then(|t| t.as_str()) != Some("file") {
            continue;
        }
        let filename = item.get("path").and_then(|p| p.as_str()).unwrap_or("").to_string();
        if filename.is_empty() {
            continue;
        }
        let size = item.get("size").and_then(|s| s.as_i64());
        let sha256 = item
            .get("lfs")
            .and_then(|l| l.get("oid"))
            .and_then(|o| o.as_str())
            .or_else(|| item.get("oid").and_then(|o| o.as_str()))
            .map(|s| s.trim_start_matches("sha256:").to_ascii_lowercase());
        let publisher = infer_publisher(&filename, repo);
        files.push(RemoteFile { repo: repo.into(), revision: revision.into(), filename, size, sha256, publisher });
    }
    Ok(files)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_serves_without_network() {
        let dir = std::env::temp_dir().join(format!("janus-hf-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cache = HfListingCache::with_client(dir.clone(), false, Box::new(BlockedHttp));
        let files = vec![RemoteFile {
            repo: "Qwen/Qwen".into(),
            revision: "main".into(),
            filename: "a-Q4_K_M.gguf".into(),
            size: Some(10),
            sha256: Some("aa".repeat(32)),
            publisher: "official".into(),
        }];
        cache.store("Qwen/Qwen", "main", Some("etag1"), &files).unwrap();
        let got = cache.fetch("Qwen/Qwen", "main").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].filename, "a-Q4_K_M.gguf");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_cache_without_opt_in_is_disabled() {
        let dir = std::env::temp_dir().join(format!("janus-hf-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cache = HfListingCache::with_client(dir.clone(), false, Box::new(BlockedHttp));
        assert_eq!(cache.fetch("Qwen/Qwen", "main").unwrap_err(), "network.disabled");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_hf_tree_lfs_oid() {
        let body = r#"[{"type":"file","path":"model-Q4_K_M.gguf","size":11,"lfs":{"oid":"ab"}}]"#;
        let files = parse_hf_tree("bartowski/Qwen", "rev1", body).unwrap();
        assert_eq!(files[0].publisher, "bartowski");
        assert_eq!(files[0].sha256.as_deref(), Some("ab"));
    }
}
