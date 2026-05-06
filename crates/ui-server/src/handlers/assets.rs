//! Static-asset fallback handler.
//!
//! Every path that didn't match a more-specific route falls through
//! here. We look the request path up in the embedded `ui/dist/`
//! handle ([`crate::UiDist`]); a hit serves the file with a guessed
//! MIME type, a miss serves `index.html` (the SPA's history
//! fallback). Anything failing both is a 404 — but in practice the
//! placeholder + future SPA always have an `index.html`, so the SPA
//! fallback is the effective behaviour for unknown routes.

use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::EmbeddedFile;

use crate::UiDist;

/// `GET /*` fallback. Serves `ui/dist/<path>` if present, else
/// `ui/dist/index.html` (SPA history fallback), else 404.
pub async fn serve_embedded(uri: Uri) -> Response {
    let raw = uri.path().trim_start_matches('/');
    let candidate = if raw.is_empty() { "index.html" } else { raw };
    if let Some(file) = UiDist::get(candidate) {
        return embedded_response(candidate, file);
    }
    if let Some(file) = UiDist::get("index.html") {
        return embedded_response("index.html", file);
    }
    (StatusCode::NOT_FOUND, "ui/dist/ is empty").into_response()
}

fn embedded_response(path: &str, file: EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(Body::from(file.data.into_owned()))
        .unwrap_or_else(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("response build failed: {e}"),
            )
                .into_response()
        })
}
