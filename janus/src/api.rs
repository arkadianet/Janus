use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use janus_core::{availability, doctor, export, fetch, hash, profile, query, radar, scan, store};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/roots", get(get_roots).post(post_root))
        .route("/api/v1/roots/discover", post(post_discover))
        .route("/api/v1/roots/{id}", delete(delete_root))
        .route("/api/v1/roots/{id}/probe", post(probe_root))
        .route("/api/v1/roots/{id}/cold", post(post_cold))
        .route("/api/v1/export", get(get_export))
        .route("/api/v1/import", post(post_import))
        .route("/api/v1/scan", post(post_scan))
        .route("/api/v1/models", get(get_models))
        .route("/api/v1/models/{id}", get(get_model))
        .route("/api/v1/files", get(get_files))
        .route("/api/v1/files/{id}", get(get_file))
        .route("/api/v1/search", get(get_search))
        .route("/api/v1/storage", get(get_storage))
        .route("/api/v1/dups", get(get_dups))
        .route("/api/v1/jobs", get(get_jobs))
        .route("/api/v1/jobs/{id}", get(get_job))
        .route("/api/v1/identify", post(post_identify))
        .route("/api/v1/merge", post(post_merge))
        .route("/api/v1/verify", post(post_verify))
        .route("/api/v1/doctor", get(get_doctor))
        .route("/api/v1/home", get(get_home))
        .route("/api/v1/profiles", get(get_profiles).put(put_profile))
        .route("/api/v1/monitors", get(get_monitors).post(post_monitor))
        .route("/api/v1/monitors/{id}", delete(delete_monitor))
        .route("/api/v1/radar", post(post_radar))
        .route("/api/v1/wanted", get(get_wanted))
        .route("/api/v1/fetch", post(post_fetch))
        .route("/api/v1/fetch/{id}", get(get_fetch))
        .route("/api/v1/fetch/{id}/pause", post(pause_fetch))
        .route("/api/v1/fetch/{id}/resume", post(resume_fetch))
        .fallback(crate::ui::static_handler)
        .with_state(state)
}

struct ApiError {
    status: StatusCode,
    code: String,
    message: String,
}

impl ApiError {
    fn code(status: StatusCode, code: &str) -> Self {
        Self { status, code: code.into(), message: code.into() }
    }
    fn from_store(e: String) -> Self {
        let status = if e.ends_with("not_found") || e == "identity.not_found" {
            StatusCode::NOT_FOUND
        } else if e == "identity.merge_declined" || e.starts_with("root.") || e == "api.bind_not_loopback" {
            StatusCode::CONFLICT
        } else if e == "network.disabled" || e.starts_with("network.disabled") {
            StatusCode::FORBIDDEN
        } else {
            StatusCode::BAD_REQUEST
        };
        Self { status, code: e.clone(), message: e }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"code": self.code, "message": self.message}))).into_response()
    }
}

fn lock(state: &AppState) -> Result<std::sync::MutexGuard<'_, Connection>, ApiError> {
    state.db.lock().map_err(|_| ApiError::code(StatusCode::INTERNAL_SERVER_ERROR, "scan.io"))
}

#[derive(Debug, Deserialize, Default)]
struct ModelQuery {
    kind: Option<String>,
    family: Option<String>,
    root: Option<String>,
    offline: Option<String>,
    dups: Option<String>,
    q: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
struct FileQuery {
    root: Option<i64>,
    state: Option<String>,
    hash_state: Option<String>,
    unknown: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
struct SearchQuery {
    q: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RootBody {
    path: String,
    kind: Option<String>,
    name: Option<String>,
    cold: Option<bool>,
    accept_marker: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ColdBody {
    cold: bool,
}

#[derive(Debug, Deserialize, Default)]
struct ScanBody {
    root_ids: Option<Vec<i64>>,
    quick: Option<bool>,
    no_hash: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct IdentifyBody {
    file_id: Option<i64>,
    path: Option<String>,
    name: String,
}

#[derive(Debug, Deserialize)]
struct MergeBody {
    src: Option<String>,
    target: Option<String>,
    decline: Option<bool>,
    a: Option<String>,
    b: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VerifyBody {
    target: String,
    full: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
struct WantedQuery {
    status: Option<String>,
    open: Option<String>,
    #[serde(rename = "have-offline")]
    have_offline: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MonitorBody {
    family: Option<String>,
    family_id: Option<i64>,
    variant_id: Option<i64>,
    profile: Option<String>,
    profile_id: Option<i64>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RadarBody {
    opt_in: Option<bool>,
    families: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct FetchBody {
    wanted_id: i64,
    force: Option<bool>,
    dest_rel_path: Option<String>,
}

async fn get_home(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let h = query::home(&conn).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(h).unwrap_or(json!({}))))
}

async fn get_roots(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let rows = query::roots(&conn).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(rows).unwrap_or(json!([]))))
}

async fn post_root(State(st): State<AppState>, Json(body): Json<RootBody>) -> Result<(StatusCode, Json<Value>), ApiError> {
    let conn = lock(&st)?;
    let kind = body.kind.unwrap_or_else(|| "internal".into());
    let name = body.name.unwrap_or_else(|| {
        std::path::Path::new(&body.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| body.path.clone())
    });
    let id = store::root_add_opts(&conn, &name, &body.path, &kind, body.accept_marker.unwrap_or(false))
        .map_err(ApiError::from_store)?;
    if body.cold.unwrap_or(false) {
        store::root_set_cold(&conn, id, true).map_err(ApiError::from_store)?;
    }
    let rows = query::roots(&conn).map_err(ApiError::from_store)?;
    let row = rows.into_iter().find(|r| r.id == id).ok_or_else(|| ApiError::code(StatusCode::NOT_FOUND, "root.not_found"))?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(row).unwrap_or(json!({})))))
}

async fn delete_root(State(st): State<AppState>, Path(id): Path<i64>) -> Result<StatusCode, ApiError> {
    let conn = lock(&st)?;
    store::root_rm(&conn, id).map_err(ApiError::from_store)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn post_discover(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let ids = store::discover_roots(&conn).map_err(ApiError::from_store)?;
    Ok(Json(json!({"ids": ids})))
}

async fn post_cold(State(st): State<AppState>, Path(id): Path<i64>, Json(body): Json<ColdBody>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    store::root_set_cold(&conn, id, body.cold).map_err(ApiError::from_store)?;
    Ok(Json(json!({"id": id, "cold": body.cold})))
}

async fn get_export(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let v = export::export(&conn).map_err(ApiError::from_store)?;
    Ok(Json(v))
}

async fn post_import(State(st): State<AppState>, Json(body): Json<Value>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let r = export::import(&conn, &body).map_err(ApiError::from_store)?;
    Ok(Json(json!({"families": r.families, "aliases": r.aliases, "declined": r.declined})))
}

async fn probe_root(State(st): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let root = store::root_by_id(&conn, id).map_err(ApiError::from_store)?;
    let now = now_secs();
    let present = store::root_probe(&conn, &root, now);
    Ok(Json(json!({"id": id, "present": present})))
}

async fn post_scan(State(st): State<AppState>, Json(body): Json<ScanBody>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let job_id = store::job_insert(&conn, "scan").map_err(ApiError::from_store)?;
    let ids = if let Some(ids) = body.root_ids.filter(|v| !v.is_empty()) {
        ids
    } else {
        store::root_ls(&conn)
            .map_err(ApiError::from_store)?
            .into_iter()
            .filter(|r| std::path::Path::new(&r.path).is_dir())
            .map(|r| r.id)
            .collect()
    };
    let quick = body.quick.unwrap_or(false) || body.no_hash.unwrap_or(false);
    let opts = scan::ScanOptions { quick };
    let mut err: Option<String> = None;
    for root_id in ids {
        if let Err(e) = scan::scan_root(&conn, root_id, &opts) {
            err = Some(e);
            break;
        }
    }
    match err {
        Some(e) => {
            store::job_finish(&conn, job_id, "error", 1.0, Some(&e)).map_err(ApiError::from_store)?;
            Err(ApiError::from_store(e))
        }
        None => {
            store::job_finish(&conn, job_id, "done", 1.0, None).map_err(ApiError::from_store)?;
            Ok(Json(json!({"job_id": job_id})))
        }
    }
}

async fn get_models(State(st): State<AppState>, Query(q): Query<ModelQuery>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let filter = query::ModelFilter {
        kind: q.kind,
        family: q.family,
        root: q.root,
        offline: truthy(&q.offline),
        dups: truthy(&q.dups),
        q: q.q,
        limit: q.limit,
    };
    let list = query::models(&conn, &filter).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(list).unwrap_or(json!({}))))
}

async fn get_model(State(st): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let detail = query::model(&conn, id).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(detail).unwrap_or(json!({}))))
}

async fn get_files(State(st): State<AppState>, Query(q): Query<FileQuery>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let filter = query::FileFilter {
        root: q.root,
        state: q.state,
        hash_state: q.hash_state,
        unknown: truthy(&q.unknown),
        limit: q.limit,
    };
    let rows = query::files_list(&conn, &filter).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(rows).unwrap_or(json!([]))))
}

async fn get_file(State(st): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let row = query::file(&conn, id).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(row).unwrap_or(json!({}))))
}

async fn get_search(State(st): State<AppState>, Query(q): Query<SearchQuery>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let hits = query::search_json(&conn, q.q.as_deref().unwrap_or("")).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(hits).unwrap_or(json!({}))))
}

async fn get_storage(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let stg = query::storage(&conn).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(stg).unwrap_or(json!({}))))
}

async fn get_dups(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let d = query::dups(&conn).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(d).unwrap_or(json!({}))))
}

async fn get_jobs(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let rows = store::job_list(&conn, 20).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(rows).unwrap_or(json!([]))))
}

async fn get_job(State(st): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let row = store::job_get(&conn, id).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(row).unwrap_or(json!({}))))
}

async fn post_identify(State(st): State<AppState>, Json(body): Json<IdentifyBody>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let id = if let Some(fid) = body.file_id {
        store::persist_manual_name_id(&conn, fid, &body.name).map_err(ApiError::from_store)?
    } else if let Some(path) = body.path {
        store::persist_manual_name(&conn, std::path::Path::new(&path), &body.name).map_err(ApiError::from_store)?
    } else {
        return Err(ApiError::code(StatusCode::BAD_REQUEST, "identity.not_found"));
    };
    Ok(Json(json!({"family_id": id})))
}

async fn post_merge(State(st): State<AppState>, Json(body): Json<MergeBody>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    if body.decline.unwrap_or(false) {
        let a = body.a.or(body.src).ok_or_else(|| ApiError::code(StatusCode::BAD_REQUEST, "identity.not_found"))?;
        let b = body.b.or(body.target).ok_or_else(|| ApiError::code(StatusCode::BAD_REQUEST, "identity.not_found"))?;
        store::declined_merge(&conn, &a, &b, janus_core::FAMILY_KEY_ALGO).map_err(ApiError::from_store)?;
        return Ok(Json(json!({"declined": true})));
    }
    let src = body.src.ok_or_else(|| ApiError::code(StatusCode::BAD_REQUEST, "identity.not_found"))?;
    let target = body.target.ok_or_else(|| ApiError::code(StatusCode::BAD_REQUEST, "identity.not_found"))?;
    let id = store::merge_families(&conn, &src, &target).map_err(ApiError::from_store)?;
    Ok(Json(json!({"family_id": id})))
}

async fn post_verify(State(st): State<AppState>, Json(body): Json<VerifyBody>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let path = if let Ok(id) = body.target.parse::<i64>() {
        store::file_abs_path(&conn, id).map_err(ApiError::from_store)?
    } else {
        std::path::PathBuf::from(&body.target)
    };
    if !body.full.unwrap_or(true) {
        return Err(ApiError::code(StatusCode::BAD_REQUEST, "hash.unverified"));
    }
    let (b3, s256, size, _) = hash::full_hash(&path).map_err(|e| ApiError::from_store(format!("scan.io: {e}")))?;
    let blob_id = store::blob_upsert(&conn, &b3, Some(&s256), size as i64, None).map_err(ApiError::from_store)?;
    Ok(Json(json!({"blake3": b3, "sha256": s256, "size": size, "blob_id": blob_id})))
}

async fn get_doctor(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let rep = doctor::report(&conn);
    Ok(Json(json!({
        "findings": rep.findings.iter().map(|f| json!({"code": f.code, "count": f.count, "message": f.message})).collect::<Vec<_>>(),
        "suggestions": rep.suggestions.iter().map(|s| json!({"a": s.a_key, "b": s.b_key, "reason": s.reason, "score": s.score})).collect::<Vec<_>>(),
    })))
}

async fn get_profiles(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let rows = query::profiles(&conn).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(rows).unwrap_or(json!([]))))
}

async fn put_profile(State(st): State<AppState>, Json(body): Json<profile::QualityProfile>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let id = profile::upsert(&conn, &body).map_err(ApiError::from_store)?;
    let row = profile::get(&conn, id).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(row).unwrap_or(json!({}))))
}

async fn get_monitors(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let rows = query::monitors(&conn).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(rows).unwrap_or(json!([]))))
}

async fn post_monitor(State(st): State<AppState>, Json(body): Json<MonitorBody>) -> Result<(StatusCode, Json<Value>), ApiError> {
    let conn = lock(&st)?;
    profile::ensure_default(&conn).map_err(ApiError::from_store)?;
    let family_id = if let Some(id) = body.family_id {
        id
    } else if let Some(name) = &body.family {
        store::family_find_id(&conn, name).ok_or_else(|| ApiError::code(StatusCode::NOT_FOUND, "identity.not_found"))?
    } else {
        return Err(ApiError::code(StatusCode::BAD_REQUEST, "identity.not_found"));
    };
    let profile_id = if let Some(id) = body.profile_id {
        id
    } else {
        let name = body.profile.as_deref().unwrap_or("daily-llm");
        profile::find_id(&conn, name).ok_or_else(|| ApiError::code(StatusCode::NOT_FOUND, "identity.not_found"))?
    };
    let id = radar::monitor_add(&conn, family_id, body.variant_id, profile_id, body.enabled.unwrap_or(true))
        .map_err(ApiError::from_store)?;
    Ok((StatusCode::CREATED, Json(json!({"id": id}))))
}

async fn delete_monitor(State(st): State<AppState>, Path(id): Path<i64>) -> Result<StatusCode, ApiError> {
    let conn = lock(&st)?;
    radar::monitor_rm(&conn, id).map_err(ApiError::from_store)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn post_radar(State(st): State<AppState>, Json(body): Json<RadarBody>) -> Result<Json<Value>, ApiError> {
    if !body.opt_in.unwrap_or(false) {
        return Err(ApiError {
            status: StatusCode::FORBIDDEN,
            code: "network.disabled".into(),
            message: radar::PRIVACY_NOTICE.into(),
        });
    }
    let conn = lock(&st)?;
    let provider = availability::live_hf(true);
    let rep = radar::sweep(
        &conn,
        &provider,
        &radar::SweepOpts { opt_in: true, families: body.families.unwrap_or_default() },
    )
    .map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(rep).unwrap_or(json!({}))))
}

async fn get_wanted(State(st): State<AppState>, Query(q): Query<WantedQuery>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let filter = radar::WantedFilter {
        status: q.status,
        open: truthy(&q.open),
        have_offline: truthy(&q.have_offline),
    };
    let out = query::wanted(&conn, &filter).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(out).unwrap_or(json!({}))))
}

async fn post_fetch(State(st): State<AppState>, Json(body): Json<FetchBody>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let res = fetch::fetch_wanted(
        &conn,
        body.wanted_id,
        body.dest_rel_path.as_deref(),
        body.force.unwrap_or(false),
        &fetch::HfHttps,
    )
    .map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(res).unwrap_or(json!({}))))
}

async fn get_fetch(State(st): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let row = fetch::task_get(&conn, id).map_err(ApiError::from_store)?;
    Ok(Json(serde_json::to_value(row).unwrap_or(json!({}))))
}

async fn pause_fetch(State(st): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let n = conn
        .execute("UPDATE fetch_tasks SET state='paused' WHERE id=?1 AND state IN ('queued','running')", [id])
        .map_err(|e| ApiError::from_store(store::to_err(e)))?;
    if n == 0 {
        return Err(ApiError::code(StatusCode::NOT_FOUND, "identity.not_found"));
    }
    Ok(Json(json!({"id": id, "state": "paused"})))
}

async fn resume_fetch(State(st): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    let conn = lock(&st)?;
    let n = conn
        .execute("UPDATE fetch_tasks SET state='queued' WHERE id=?1 AND state='paused'", [id])
        .map_err(|e| ApiError::from_store(store::to_err(e)))?;
    if n == 0 {
        return Err(ApiError::code(StatusCode::NOT_FOUND, "identity.not_found"));
    }
    Ok(Json(json!({"id": id, "state": "queued"})))
}

fn truthy(v: &Option<String>) -> bool {
    matches!(v.as_deref(), Some("1") | Some("true") | Some("yes") | Some("on"))
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
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use janus_core::db;
    use tower::ServiceExt;

    fn app() -> Router {
        let conn = db::open(None).unwrap();
        db::init_schema(&conn).unwrap();
        router(AppState { db: Arc::new(Mutex::new(conn)) })
    }

    async fn call(app: Router, req: Request<Body>) -> (StatusCode, Value) {
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v = if bytes.is_empty() {
            json!(null)
        } else {
            serde_json::from_slice(&bytes).unwrap_or(json!(String::from_utf8_lossy(&bytes).into_owned()))
        };
        (status, v)
    }

    #[tokio::test]
    async fn models_matches_query_engine() {
        let conn = db::open(None).unwrap();
        db::init_schema(&conn).unwrap();
        let id = store::family_insert(&conn, "foo|llama|t8|a8", Some("Foo"), Some("llama"), Some(8.0), None, None, "llm").unwrap();
        store::evidence_put(&conn, "family", id, "name", "Foo", "inferred", "filename");
        let via_query = query::models(&conn, &query::ModelFilter::default()).unwrap();
        let state = AppState { db: Arc::new(Mutex::new(conn)) };
        let (status, body) = call(
            router(state),
            Request::builder().uri("/api/v1/models").body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["counts"]["families"], via_query.counts.families);
        assert_eq!(body["counts"]["families_inferred"], via_query.counts.families_inferred);
        assert_eq!(body["families"][0]["name"]["level"], "inferred");
        assert_eq!(body["families"][0]["name"]["value"], "Foo");
    }

    #[tokio::test]
    async fn empty_home_has_first_run_copy() {
        let (status, body) = call(
            app(),
            Request::builder().uri("/").body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let html = body.as_str().unwrap_or("");
        assert!(html.contains("Add a folder you already keep models in"), "{html}");
        assert!(html.contains("will not move"), "{html}");
    }

    #[tokio::test]
    async fn catalogue_reads_are_wired() {
        let r = app();
        for path in ["/api/v1/roots", "/api/v1/models", "/api/v1/files", "/api/v1/storage", "/api/v1/dups", "/api/v1/search?q=x", "/api/v1/jobs", "/api/v1/profiles", "/api/v1/monitors", "/api/v1/wanted", "/api/v1/export"] {
            let (status, _) = call(r.clone(), Request::builder().uri(path).body(Body::empty()).unwrap()).await;
            assert_eq!(status, StatusCode::OK, "{path}");
        }
    }

    #[tokio::test]
    async fn add_root_and_home_counts() {
        let dir = std::env::temp_dir().join(format!("janus-api-root-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let conn = db::open(None).unwrap();
        db::init_schema(&conn).unwrap();
        let state = AppState { db: Arc::new(Mutex::new(conn)) };
        let r = router(state);
        let (status, body) = call(
            r.clone(),
            Request::builder()
                .method("POST")
                .uri("/api/v1/roots")
                .header("content-type", "application/json")
                .body(Body::from(json!({"path": dir.to_string_lossy(), "name": "models", "accept_marker": true}).to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["name"], "models");
        assert_eq!(body["writable"], false);
        let (status, home) = call(r, Request::builder().uri("/api/v1/home").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(home["counts"]["roots"], 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn scan_returns_job_id() {
        let dir = std::env::temp_dir().join(format!("janus-api-scan-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let conn = db::open(None).unwrap();
        db::init_schema(&conn).unwrap();
        store::root_add_opts(&conn, "models", dir.to_str().unwrap(), "internal", true).unwrap();
        let state = AppState { db: Arc::new(Mutex::new(conn)) };
        let (status, body) = call(
            router(state),
            Request::builder()
                .method("POST")
                .uri("/api/v1/scan")
                .header("content-type", "application/json")
                .body(Body::from(json!({"quick": true}).to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body["job_id"].as_i64().unwrap() >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn radar_refuses_without_opt_in() {
        let (status, body) = call(
            app(),
            Request::builder()
                .method("POST")
                .uri("/api/v1/radar")
                .header("content-type", "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["code"], "network.disabled");
        assert!(body["message"].as_str().unwrap_or("").contains("Hugging Face"));
    }

    #[tokio::test]
    async fn wanted_names_what_leaves_the_machine() {
        let (status, body) = call(app(), Request::builder().uri("/api/v1/wanted").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::OK);
        let notice = body["privacy_notice"].as_str().unwrap_or("");
        assert!(notice.contains("Hugging Face"), "{notice}");
        assert!(notice.contains("weights do not leave"), "{notice}");
        assert!(body["items"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn fetch_rejects_traversal_and_null_digest() {
        let dir = std::env::temp_dir().join(format!("janus-api-fetch-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let conn = db::open(None).unwrap();
        db::init_schema(&conn).unwrap();
        store::root_add_opts(&conn, "inbound", dir.to_str().unwrap(), "fetch", true).unwrap();
        conn.execute(
            "INSERT INTO wanted_items (remote_key, provider, repo, revision, filename, sha256, status)
             VALUES ('hf|acme/x|main|a.gguf','hf','acme/x','main','a.gguf',NULL,'open')",
            [],
        )
        .unwrap();
        let state = AppState { db: Arc::new(Mutex::new(conn)) };
        let r = router(state);
        let (status, body) = call(
            r.clone(),
            Request::builder()
                .method("POST")
                .uri("/api/v1/fetch")
                .header("content-type", "application/json")
                .body(Body::from(json!({"wanted_id": 1, "dest_rel_path": "../escape.gguf"}).to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["code"], "fetch.path_invalid");
        let (status, body) = call(
            r,
            Request::builder()
                .method("POST")
                .uri("/api/v1/fetch")
                .header("content-type", "application/json")
                .body(Body::from(json!({"wanted_id": 1, "dest_rel_path": "a.gguf"}).to_string()))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["code"], "wanted.no_sha256");
        assert!(!dir.join("a.gguf").exists());
        assert!(!dir.parent().unwrap().join("escape.gguf").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn unknown_model_is_not_found() {
        let (status, body) = call(
            app(),
            Request::builder().uri("/api/v1/models/99").body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["code"], "identity.not_found");
    }
}
