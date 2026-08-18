use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "ui/"]
struct Assets;

pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let file = if is_spa(path) { "index.html" } else { path };
    match Assets::get(file) {
        Some(f) => {
            let mime = content_type(file);
            ([(header::CONTENT_TYPE, mime)], f.data).into_response()
        }
        None => match Assets::get("index.html") {
            Some(f) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], f.data).into_response(),
            None => (StatusCode::NOT_FOUND, "ui missing").into_response(),
        },
    }
}

fn is_spa(path: &str) -> bool {
    path.is_empty()
        || path == "library"
        || path == "unknown"
        || path == "storage"
        || path == "search"
        || path == "wanted"
        || path.starts_with("model/")
}

fn content_type(path: &str) -> &'static str {
    if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "text/html; charset=utf-8"
    }
}
