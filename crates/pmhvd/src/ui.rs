use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};

/// The built web UI. Read from disk in debug builds and compiled in for release builds.
#[derive(rust_embed::Embed)]
#[folder = "../../web/dist"]
#[allow_missing = true]
struct Assets;

const INDEX: &str = "index.html";

/// Serves a UI asset, or `index.html` for any other path so client-side routes resolve.
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    asset(path)
        .or_else(|| asset(INDEX))
        .unwrap_or_else(not_built)
}

fn asset(path: &str) -> Option<Response> {
    let file = Assets::get(path)?;
    // Vite fingerprints everything under assets/, so those never change.
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Some(
        (
            [
                (CONTENT_TYPE, file.metadata.mimetype().to_owned()),
                (CACHE_CONTROL, cache.to_owned()),
            ],
            file.data,
        )
            .into_response(),
    )
}

fn not_built() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "The web UI hasn't been built into this pmhvd. Run `just build-web`, then restart pmhvd.\n",
    )
        .into_response()
}
