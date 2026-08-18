use crate::api::{self, AppState};
use janus_core::bind::{check_bind, load_daemon_config, BindAddr, Expose};
use janus_core::{db, store, writer};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

pub fn prepare_bind(spec: &str, expose: &Expose) -> Result<BindAddr, String> {
    check_bind(spec, expose)
}

fn open_db() -> Connection {
    let p = store::db_path();
    let conn = db::open(Some(&p)).expect("open db");
    db::require_schema(&conn).expect("schema");
    conn
}

pub async fn run(api_flag: Option<&str>) -> Result<(), String> {
    let (cfg_api, expose) = load_daemon_config();
    let spec = api_flag.unwrap_or(cfg_api.as_str());
    let addr = prepare_bind(spec, &expose)?;
    let listener = TcpListener::bind(addr.socket_addr()).await.map_err(|e| format!("bind: {e}"))?;
    writer::write_info(&addr.socket_addr())?;
    let state = AppState { db: Arc::new(Mutex::new(open_db())) };
    let app = api::router(state);
    println!("janus daemon  http://{}", addr.socket_addr());
    println!("UI            http://{}/", addr.socket_addr());
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;
    writer::clear_info();
    result.map_err(|e| e.to_string())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_refuses_wildcard() {
        assert_eq!(
            prepare_bind("0.0.0.0:4321", &Expose::off()).unwrap_err(),
            "api.bind_not_loopback"
        );
        assert!(prepare_bind("127.0.0.1:4321", &Expose::off()).is_ok());
    }
}
