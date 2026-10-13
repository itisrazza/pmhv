use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{OriginalUri, Path, State, WebSocketUpgrade};
use axum::http::{StatusCode, header};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::Stream;
use futures_util::StreamExt;
use pmhv_model::report::error_chain;
use pmhv_model::{
    ApiError, CreateMachineRequest, Event, HostInfo, ImageGroup, Machine, MachineId, MachineSpec,
    PowerRequest, ThumbnailKind,
};
use tokio::net::UnixStream;
use tokio::sync::watch;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use utoipa::OpenApi;

use crate::console;
use crate::images::ImageError;
use crate::machines::{MachineError, Machines};

#[derive(OpenApi)]
#[openapi(
    info(title = "pmhv", description = "Poor Man's HyperVisor daemon API"),
    paths(
        host_info,
        list_machines,
        create_machine,
        get_machine,
        update_machine,
        delete_machine,
        power,
        list_images
    ),
    components(schemas(ApiError, Event))
)]
pub struct ApiDoc;

#[derive(Clone)]
pub struct AppState {
    pub machines: Arc<Machines>,
    /// Flips to true on shutdown, so endless responses (the event stream) can finish.
    pub shutdown: watch::Receiver<bool>,
}

pub fn router(state: AppState) -> Router {
    let v1 = Router::new()
        .route("/host", get(host_info))
        .route("/machines", get(list_machines).post(create_machine))
        .route(
            "/machines/{id}",
            get(get_machine).put(update_machine).delete(delete_machine),
        )
        .route("/machines/{id}/power", post(power))
        .route("/machines/{id}/console/serial", get(serial_console))
        .route("/machines/{id}/console/vnc", get(vnc_console))
        .route("/machines/{id}/thumbnail", get(thumbnail))
        .route("/images", get(list_images))
        .route("/events", get(events));
    Router::new()
        .nest("/api", Router::new().nest("/v1", v1).fallback(not_found))
        .with_state(state)
}

/// An error as the API reports it.
pub struct ApiFailure {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl IntoResponse for ApiFailure {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            tracing::error!(code = self.code, message = self.message, "request failed");
        }
        let body = ApiError {
            code: self.code.to_owned(),
            message: self.message,
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<MachineError> for ApiFailure {
    fn from(error: MachineError) -> Self {
        let message = error_chain(&error);
        let (status, code) = match &error {
            MachineError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            MachineError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            MachineError::Invalid(_) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_spec"),
            MachineError::Start(_) | MachineError::Shim(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "machine_error")
            }
            MachineError::Image(error) => return image_failure(error, message),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        Self {
            status,
            code,
            message,
        }
    }
}

impl From<ImageError> for ApiFailure {
    fn from(error: ImageError) -> Self {
        let message = error_chain(&error);
        image_failure(&error, message)
    }
}

fn image_failure(error: &ImageError, message: String) -> ApiFailure {
    let (status, code) = match error {
        ImageError::UnknownGroup(_) | ImageError::NotFound { .. } => {
            (StatusCode::NOT_FOUND, "not_found")
        }
        ImageError::AlreadyExists { .. } => (StatusCode::CONFLICT, "conflict"),
        ImageError::InvalidName(_) | ImageError::UnknownFormat { .. } => {
            (StatusCode::UNPROCESSABLE_ENTITY, "invalid_image")
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    ApiFailure {
        status,
        code,
        message,
    }
}

type ApiResult<T> = Result<T, ApiFailure>;

/// Facts about the host pmhvd runs on.
#[utoipa::path(get, path = "/api/v1/host", operation_id = "getHost", tag = "host",
    responses((status = 200, description = "Host facts", body = HostInfo)))]
async fn host_info() -> Json<HostInfo> {
    Json(HostInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
    })
}

#[utoipa::path(get, path = "/api/v1/machines", operation_id = "listMachines", tag = "machines",
    responses((status = 200, description = "All machines, by name", body = [Machine])))]
async fn list_machines(State(AppState { machines, .. }): State<AppState>) -> Json<Vec<Machine>> {
    Json(machines.list())
}

#[utoipa::path(post, path = "/api/v1/machines", operation_id = "createMachine", tag = "machines",
    request_body = CreateMachineRequest,
    responses(
        (status = 201, description = "Created", body = Machine),
        (status = 422, description = "Invalid spec", body = ApiError),
    ))]
async fn create_machine(
    State(AppState { machines, .. }): State<AppState>,
    Json(request): Json<CreateMachineRequest>,
) -> ApiResult<(StatusCode, Json<Machine>)> {
    Ok((StatusCode::CREATED, Json(machines.create(request).await?)))
}

#[utoipa::path(get, path = "/api/v1/machines/{id}", operation_id = "getMachine", tag = "machines",
    params(("id" = String, Path, description = "Machine ID")),
    responses(
        (status = 200, description = "The machine", body = Machine),
        (status = 404, description = "No such machine", body = ApiError),
    ))]
async fn get_machine(
    State(AppState { machines, .. }): State<AppState>,
    Path(id): Path<MachineId>,
) -> ApiResult<Json<Machine>> {
    Ok(Json(machines.get(id)?))
}

/// Replaces a stopped machine's spec.
#[utoipa::path(put, path = "/api/v1/machines/{id}", operation_id = "updateMachine", tag = "machines",
    params(("id" = String, Path, description = "Machine ID")),
    request_body = MachineSpec,
    responses(
        (status = 200, description = "Updated", body = Machine),
        (status = 409, description = "The machine isn't stopped", body = ApiError),
        (status = 422, description = "Invalid spec", body = ApiError),
    ))]
async fn update_machine(
    State(AppState { machines, .. }): State<AppState>,
    Path(id): Path<MachineId>,
    Json(spec): Json<MachineSpec>,
) -> ApiResult<Json<Machine>> {
    Ok(Json(machines.update(id, spec)?))
}

/// Deletes a stopped machine. Its disk images stay in the image store.
#[utoipa::path(delete, path = "/api/v1/machines/{id}", operation_id = "deleteMachine", tag = "machines",
    params(("id" = String, Path, description = "Machine ID")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 409, description = "The machine isn't stopped", body = ApiError),
    ))]
async fn delete_machine(
    State(AppState { machines, .. }): State<AppState>,
    Path(id): Path<MachineId>,
) -> ApiResult<StatusCode> {
    machines.delete(id)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/api/v1/machines/{id}/power", operation_id = "powerMachine", tag = "machines",
    params(("id" = String, Path, description = "Machine ID")),
    request_body = PowerRequest,
    responses(
        (status = 200, description = "Action accepted; state changes follow on the event stream", body = Machine),
        (status = 409, description = "Not possible in the machine's current state", body = ApiError),
        (status = 422, description = "The machine couldn't start or refused the action", body = ApiError),
    ))]
async fn power(
    State(AppState { machines, .. }): State<AppState>,
    Path(id): Path<MachineId>,
    Json(request): Json<PowerRequest>,
) -> ApiResult<Json<Machine>> {
    Ok(Json(machines.power(id, request.action).await?))
}

#[utoipa::path(get, path = "/api/v1/images", operation_id = "listImages", tag = "images",
    responses((status = 200, description = "Storage groups and their images", body = [ImageGroup])))]
async fn list_images(
    State(AppState { machines, .. }): State<AppState>,
) -> ApiResult<Json<Vec<ImageGroup>>> {
    Ok(Json(machines.images().list().await?))
}

async fn serial_console(
    State(AppState { machines, .. }): State<AppState>,
    Path(id): Path<MachineId>,
    upgrade: WebSocketUpgrade,
) -> ApiResult<Response> {
    let path = machines.console_socket(id)?;
    proxy(upgrade, &path, machines, id).await
}

async fn vnc_console(
    State(AppState { machines, .. }): State<AppState>,
    Path(id): Path<MachineId>,
    upgrade: WebSocketUpgrade,
) -> ApiResult<Response> {
    let path = machines.vnc_socket(id)?;
    // noVNC asks for the "binary" subprotocol.
    proxy(upgrade.protocols(["binary"]), &path, machines, id).await
}

async fn proxy(
    upgrade: WebSocketUpgrade,
    path: &std::path::Path,
    machines: Arc<Machines>,
    id: MachineId,
) -> ApiResult<Response> {
    let stream = UnixStream::connect(path)
        .await
        .map_err(|error| ApiFailure {
            status: StatusCode::BAD_GATEWAY,
            code: "console_unavailable",
            message: format!(
                "can't connect to the console at {}: {error}",
                path.display()
            ),
        })?;
    Ok(upgrade.on_upgrade(move |socket| async move {
        console::pipe(socket, stream).await;
        // Someone just looked at the screen; make the card show what they last saw.
        machines.capture_thumbnail(id).await;
    }))
}

/// The machine's last screen capture: a PNG, or plain text for text consoles.
async fn thumbnail(
    State(AppState { machines, .. }): State<AppState>,
    Path(id): Path<MachineId>,
) -> ApiResult<Response> {
    let Some((thumbnail, path)) = machines.thumbnail_file(id)? else {
        return Err(ApiFailure {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: "no thumbnail captured yet".to_owned(),
        });
    };
    let body = tokio::fs::read(&path).await.map_err(|error| ApiFailure {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "internal",
        message: format!("failed to read {}: {error}", path.display()),
    })?;
    let content_type = match thumbnail.kind {
        ThumbnailKind::Image => "image/png",
        ThumbnailKind::Text => "text/plain; charset=utf-8",
    };
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        body,
    )
        .into_response())
}

/// Server-sent events: machine changes, plus `lagged` if this client fell behind and should refetch.
async fn events(
    State(AppState { machines, shutdown }): State<AppState>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let mut shutdown = shutdown;
    let stopping = async move {
        // An error means the sender's gone, which only happens during shutdown anyway.
        let _ = shutdown.wait_for(|stop| *stop).await;
    };
    let stream = BroadcastStream::new(machines.subscribe()).filter_map(|item| async move {
        match item {
            Ok(event) => SseEvent::default()
                .json_data(event)
                .inspect_err(|error| tracing::error!(%error, "failed to encode event"))
                .ok()
                .map(Ok),
            Err(BroadcastStreamRecvError::Lagged(_)) => {
                Some(Ok(SseEvent::default().event("lagged").data("")))
            }
        }
    });
    Sse::new(stream.take_until(stopping)).keep_alive(KeepAlive::default())
}

async fn not_found(OriginalUri(uri): OriginalUri) -> ApiFailure {
    ApiFailure {
        status: StatusCode::NOT_FOUND,
        code: "not_found",
        message: format!("no API route for {}", uri.path()),
    }
}
