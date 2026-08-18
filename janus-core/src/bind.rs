//! Daemon bind policy: loopback only unless expose (auth + TLS + origins) is complete.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expose {
    pub auth: Option<String>,
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
    pub allow_origins: Vec<String>,
}

impl Expose {
    pub fn off() -> Self {
        Self::default()
    }

    pub fn is_complete(&self) -> bool {
        nonempty(&self.auth) && nonempty(&self.tls_cert) && nonempty(&self.tls_key) && !self.allow_origins.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindAddr {
    pub host: String,
    pub port: u16,
}

impl BindAddr {
    pub fn socket_addr(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

pub fn parse_bind(spec: &str) -> Result<BindAddr, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("api.bind_not_loopback".into());
    }
    if let Some(rest) = spec.strip_prefix('[') {
        let (host, port) = rest.split_once("]:").ok_or_else(|| "api.bind_not_loopback".to_string())?;
        let port: u16 = port.parse().map_err(|_| "api.bind_not_loopback".to_string())?;
        return Ok(BindAddr { host: host.to_string(), port });
    }
    let (host, port) = spec.rsplit_once(':').ok_or_else(|| "api.bind_not_loopback".to_string())?;
    let port: u16 = port.parse().map_err(|_| "api.bind_not_loopback".to_string())?;
    Ok(BindAddr { host: host.to_string(), port })
}

pub fn check_bind(spec: &str, expose: &Expose) -> Result<BindAddr, String> {
    let addr = parse_bind(spec)?;
    if is_loopback(&addr.host) {
        return Ok(addr);
    }
    if expose.is_complete() {
        return Ok(addr);
    }
    Err("api.bind_not_loopback".into())
}

pub fn is_loopback(host: &str) -> bool {
    let h = host.trim().trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
    if h == "localhost" || h == "::1" {
        return true;
    }
    if let Ok(ip) = h.parse::<std::net::Ipv4Addr>() {
        return ip.is_loopback();
    }
    if let Ok(ip) = h.parse::<std::net::Ipv6Addr>() {
        return ip.is_loopback();
    }
    false
}

fn nonempty(s: &Option<String>) -> bool {
    s.as_deref().map(|v| !v.trim().is_empty()).unwrap_or(false)
}

pub fn config_from_toml(raw: &str) -> (Option<String>, Expose) {
    let v: toml::Value = toml::from_str(raw).unwrap_or_else(|_| toml::Value::Table(Default::default()));
    let daemon = v.get("daemon");
    let api = daemon.and_then(|d| d.get("api")).and_then(|a| a.as_str()).map(|s| s.to_string());
    let ex = daemon.and_then(|d| d.get("expose"));
    let expose = Expose {
        auth: ex.and_then(|e| e.get("auth")).and_then(|a| a.as_str()).map(|s| s.to_string()),
        tls_cert: ex.and_then(|e| e.get("tls_cert")).and_then(|a| a.as_str()).map(|s| s.to_string()),
        tls_key: ex.and_then(|e| e.get("tls_key")).and_then(|a| a.as_str()).map(|s| s.to_string()),
        allow_origins: ex
            .and_then(|e| e.get("allow_origins"))
            .and_then(|a| a.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default(),
    };
    (api, expose)
}

pub fn load_daemon_config() -> (String, Expose) {
    let default = "127.0.0.1:4321".to_string();
    let path = dirs::config_dir().map(|d| d.join("janus").join("config.toml"));
    let Some(path) = path else {
        return (default, Expose::off());
    };
    let Ok(raw) = std::fs::read_to_string(path) else {
        return (default, Expose::off());
    };
    let (api, expose) = config_from_toml(&raw);
    (api.unwrap_or(default), expose)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_is_allowed() {
        assert!(check_bind("127.0.0.1:4321", &Expose::off()).is_ok());
        assert!(check_bind("127.0.0.2:9", &Expose::off()).is_ok());
        assert!(check_bind("localhost:4321", &Expose::off()).is_ok());
        assert!(check_bind("[::1]:4321", &Expose::off()).is_ok());
        assert!(check_bind("::1:4321", &Expose::off()).is_ok());
    }

    #[test]
    fn wildcard_rejected_without_expose() {
        assert_eq!(check_bind("0.0.0.0:4321", &Expose::off()).unwrap_err(), "api.bind_not_loopback");
        assert_eq!(check_bind("[::]:80", &Expose::off()).unwrap_err(), "api.bind_not_loopback");
        assert_eq!(check_bind(":::80", &Expose::off()).unwrap_err(), "api.bind_not_loopback");
    }

    #[test]
    fn lan_rejected_unless_expose_complete() {
        let partial = Expose {
            auth: Some("token".into()),
            tls_cert: Some("cert.pem".into()),
            tls_key: None,
            allow_origins: vec!["http://127.0.0.1:4321".into()],
        };
        assert_eq!(
            check_bind("192.168.1.5:4321", &partial).unwrap_err(),
            "api.bind_not_loopback"
        );
        let full = Expose {
            auth: Some("token".into()),
            tls_cert: Some("cert.pem".into()),
            tls_key: Some("key.pem".into()),
            allow_origins: vec!["https://example.local".into()],
        };
        assert!(check_bind("192.168.1.5:4321", &full).is_ok());
    }

    #[test]
    fn default_spec_is_loopback() {
        let a = check_bind("127.0.0.1:4321", &Expose::off()).unwrap();
        assert_eq!(a.host, "127.0.0.1");
        assert_eq!(a.port, 4321);
        assert_eq!(a.socket_addr(), "127.0.0.1:4321");
    }

    #[test]
    fn expose_from_toml_is_complete_only_with_trio() {
        let raw = r#"
[daemon]
api = "127.0.0.1:9"
[daemon.expose]
auth = "token"
tls_cert = "c.pem"
tls_key = "k.pem"
allow_origins = ["http://127.0.0.1:9"]
"#;
        let (api, ex) = config_from_toml(raw);
        assert_eq!(api.as_deref(), Some("127.0.0.1:9"));
        assert!(ex.is_complete());
        let (_, incomplete) = config_from_toml("[daemon]\napi=\"0.0.0.0:1\"\n");
        assert!(!incomplete.is_complete());
    }
}
