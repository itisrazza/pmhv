use axum::extract::OriginalUri;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use pmhv_model::{ApiError, HostInfo};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(title = "pmhv", description = "Poor Man's HyperVisor daemon API"),
    paths(host_info),
    components(schemas(ApiError))
)]
pub struct ApiDoc;

pub fn router() -> Router {
    let v1 = Router::new().route("/host", get(host_info));
    Router::new().nest("/api", Router::new().nest("/v1", v1).fallback(not_found))
}

/// Facts about the host pmhvd runs on.
#[utoipa::path(
    get,
    path = "/api/v1/host",
    operation_id = "getHost",
    tag = "host",
    responses((status = 200, description = "Host facts", body = HostInfo))
)]
async fn host_info() -> Json<HostInfo> {
    Json(HostInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
    })
}

async fn not_found(OriginalUri(uri): OriginalUri) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            code: "not_found".to_owned(),
            message: format!("no API route for {}", uri.path()),
        }),
    )
}
