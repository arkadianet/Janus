//! Advisory single-writer: when the daemon is live, it owns SQLite writes.

use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

use crate::store;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DaemonInfo {
    pub pid: u32,
    pub api: String,
}

pub fn info_path() -> PathBuf {
    store::db_path().with_file_name("daemon.json")
}

pub fn write_info(api: &str) -> Result<(), String> {
    let info = DaemonInfo { pid: std::process::id(), api: api.to_string() };
    let path = info_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(&info).unwrap_or_default()).map_err(|e| e.to_string())
}

pub fn clear_info() {
    let _ = std::fs::remove_file(info_path());
}

pub fn live_daemon() -> Option<String> {
    let raw = std::fs::read_to_string(info_path()).ok()?;
    let info: DaemonInfo = serde_json::from_str(&raw).ok()?;
    let addr = info.api.trim();
    let hostport = addr.rsplit_once(':')?;
    let ip: std::net::IpAddr = hostport.0.parse().ok()?;
    let port: u16 = hostport.1.parse().ok()?;
    TcpStream::connect_timeout(&(ip, port).into(), Duration::from_millis(150))
        .ok()
        .map(|_| format!("http://{addr}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_info_is_not_live() {
        if !info_path().exists() {
            assert!(live_daemon().is_none());
        }
    }
}
