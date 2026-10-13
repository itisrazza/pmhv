use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, OriginalUri, Path, Request, State, WebSocketUpgrade};
use std::time::Duration;

use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::Stream;
use futures_util::StreamExt;
use pmhv_model::report::error_chain;
use pmhv_model::{
    ApiError, AuthStatus, CreateImageRequest, CreateMachineRequest, Event, HostInfo, ImageGroup,
    ImageRef, LoginRequest, Machine, MachineId, MachineSpec, PowerRequest, ResizeImageRequest,
    SetPasswordRequest, TemporaryPassword, ThumbnailKind,
};
use tokio::net::UnixStream;
use tokio::sync::watch;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use utoipa::OpenApi;

use crate::access;
use crate::auth::{Auth, AuthError, SESSION_LIFETIME, SessionKind};
use crate::console;
use crate::images::ImageError;
use crate::machines::{MachineError, Machines};

#[derive(OpenApi)]
#[openapi(
    info(title = "pmhv", description = "Poor Man's HyperVisor daemon API"),
    paths(
        login,
        logout,
        session,
        set_password,
        host_info,
        list_machines,
        create_machine,
        get_machine,
        update_machine,
        delete_machine,
        power,
        list_images,
        create_image,
        upload_image,
        delete_image,
        resize_image
    ),
    components(schemas(ApiError, Event))
)]
pub struct ApiDoc;

#[derive(Clone)]
pub struct AppState {
    pub machines: Arc<Machines>,
    pub auth: Arc<Auth>,
    /// Requests over the local control socket, which filesystem permissions already restrict.
    pub local: bool,
    /// Flips to true on shutdown, so endless responses (the event stream) can finish.
    pub shutdown: watch::Receiver<bool>,
}

pub fn router(state: AppState) -> Router {
    let mut v1 = Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/session", get(session))
        .route("/auth/password", post(set_password))
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
        .route("/images", get(list_images).post(create_image))
        .route(
            "/images/{group}/{name}",
            get(download_image)
                .put(upload_image)
                .delete(delete_image)
                // Images are routinely gigabytes.
                .layer(DefaultBodyLimit::disable()),
        )
        .route("/images/{group}/{name}/resize", post(resize_image))
        .route("/events", get(events));
    if state.local {
        v1 = v1.route("/auth/reset", post(reset_auth));
    }
    let api = Router::new().nest("/api", Router::new().nest("/v1", v1).fallback(not_found));
    let api = if state.local {
        api
    } else {
        api.layer(middleware::from_fn_with_state(
            state.clone(),
            access::require_session,
        ))
        .layer(middleware::from_fn(access::same_origin))
    };
    api.with_state(state)
}

/// An error as the API reports it.
pub struct ApiFailure {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiFailure {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
}

impl From<AuthError> for ApiFailure {
    fn from(error: AuthError) -> Self {
        let (status, code) = match &error {
            AuthError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            AuthError::NoSession => (StatusCode::UNAUTHORIZED, "unauthenticated"),
            AuthError::PasswordTooShort => (StatusCode::UNPROCESSABLE_ENTITY, "weak_password"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        Self::new(status, code, error_chain(&error))
    }
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
        ImageError::AlreadyExists { .. } | ImageError::InUse { .. } => {
            (StatusCode::CONFLICT, "conflict")
        }
        ImageError::InvalidName(_)
        | ImageError::UnknownFormat { .. }
        | ImageError::WouldShrink { .. }
        | ImageError::ResizeIso => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_image"),
        ImageError::Upload(_) => (StatusCode::BAD_REQUEST, "upload_failed"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    ApiFailure {
        status,
        code,
        message,
    }
}

type ApiResult<T> = Result<T, ApiFailure>;

/// Slows down password guessing without making a typo painful.
const FAILED_LOGIN_DELAY: Duration = Duration::from_millis(750);

/// Signs in with the password (or the temporary one, which only allows setting a password).
#[utoipa::path(post, path = "/api/v1/auth/login", operation_id = "login", tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Signed in; the session cookie is set", body = AuthStatus),
        (status = 401, description = "Wrong password", body = ApiError),
    ))]
async fn login(
    State(AppState { auth, .. }): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> ApiResult<Response> {
    let (token, kind) = match auth.login(&request.password).await {
        Ok(session) => session,
        Err(error) => {
            tokio::time::sleep(FAILED_LOGIN_DELAY).await;
            return Err(error.into());
        }
    };
    let status = AuthStatus {
        authenticated: true,
        setup_required: kind == SessionKind::Setup,
    };
    Ok((
        [(
            header::SET_COOKIE,
            access::session_cookie(&token, SESSION_LIFETIME.as_secs()),
        )],
        Json(status),
    )
        .into_response())
}

#[utoipa::path(post, path = "/api/v1/auth/logout", operation_id = "logout", tag = "auth",
    responses((status = 204, description = "Signed out; the session cookie is cleared")))]
async fn logout(State(AppState { auth, .. }): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = access::session_token(&headers) {
        auth.logout(&token);
    }
    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, access::session_cookie("", 0))],
    )
        .into_response()
}

#[utoipa::path(get, path = "/api/v1/auth/session", operation_id = "getSession", tag = "auth",
    responses((status = 200, description = "Whether this browser is signed in", body = AuthStatus)))]
async fn session(
    State(AppState { auth, local, .. }): State<AppState>,
    headers: HeaderMap,
) -> Json<AuthStatus> {
    let kind = access::session_token(&headers).and_then(|token| auth.session(&token));
    Json(AuthStatus {
        authenticated: local || kind.is_some(),
        setup_required: !local && kind == Some(SessionKind::Setup),
    })
}

/// Sets the password. Other sessions are signed out.
#[utoipa::path(post, path = "/api/v1/auth/password", operation_id = "setPassword", tag = "auth",
    request_body = SetPasswordRequest,
    responses(
        (status = 200, description = "Password set", body = AuthStatus),
        (status = 422, description = "Password too short", body = ApiError),
    ))]
async fn set_password(
    State(AppState { auth, .. }): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SetPasswordRequest>,
) -> ApiResult<Json<AuthStatus>> {
    let token = access::session_token(&headers).ok_or(AuthError::NoSession)?;
    auth.set_password(&token, &request.password).await?;
    Ok(Json(AuthStatus {
        authenticated: true,
        setup_required: false,
    }))
}

/// Local socket only: forget the password and issue a new temporary one.
async fn reset_auth(
    State(AppState { auth, .. }): State<AppState>,
) -> ApiResult<Json<TemporaryPassword>> {
    Ok(Json(TemporaryPassword {
        password: auth.reset()?,
    }))
}

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
    Ok(Json(machines.list_images().await?))
}

#[utoipa::path(post, path = "/api/v1/images", operation_id = "createImage", tag = "images",
    request_body = CreateImageRequest,
    responses(
        (status = 201, description = "Created", body = ImageRef),
        (status = 409, description = "An image with that name exists", body = ApiError),
    ))]
async fn create_image(
    State(AppState { machines, .. }): State<AppState>,
    Json(request): Json<CreateImageRequest>,
) -> ApiResult<(StatusCode, Json<ImageRef>)> {
    let image = ImageRef {
        group: request.group,
        name: request.name,
    };
    machines
        .images()
        .create_blank(&image, request.size_gib)
        .await?;
    Ok((StatusCode::CREATED, Json(image)))
}

/// Uploads an image file as the request body. Existing images aren't replaced.
#[utoipa::path(put, path = "/api/v1/images/{group}/{name}", operation_id = "uploadImage", tag = "images",
    params(("group" = String, Path), ("name" = String, Path)),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 201, description = "Uploaded", body = ImageRef),
        (status = 409, description = "An image with that name exists", body = ApiError),
    ))]
async fn upload_image(
    State(AppState { machines, .. }): State<AppState>,
    Path((group, name)): Path<(String, String)>,
    body: Body,
) -> ApiResult<(StatusCode, Json<ImageRef>)> {
    let image = ImageRef { group, name };
    machines
        .images()
        .upload(&image, body.into_data_stream())
        .await?;
    tracing::info!(group = image.group, name = image.name, "image uploaded");
    Ok((StatusCode::CREATED, Json(image)))
}

/// Downloads an image. Supports `Range` requests, so interrupted downloads can resume.
async fn download_image(
    State(AppState { machines, .. }): State<AppState>,
    Path((group, name)): Path<(String, String)>,
    request: Request,
) -> ApiResult<Response> {
    let image = ImageRef { group, name };
    let (path, _) = machines.images().resolve(&image)?;
    let response = ServeFile::new(&path)
        .oneshot(request)
        .await
        .map_err(|error| {
            ApiFailure::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                error.to_string(),
            )
        })?;
    let disposition = format!("attachment; filename=\"{}\"", image.name.replace('"', "'"));
    let mut response = response.map(Body::new);
    if let Ok(value) = header::HeaderValue::from_str(&disposition) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}

/// Deletes an image that no machine has attached.
#[utoipa::path(delete, path = "/api/v1/images/{group}/{name}", operation_id = "deleteImage", tag = "images",
    params(("group" = String, Path), ("name" = String, Path)),
    responses(
        (status = 204, description = "Deleted"),
        (status = 409, description = "A machine uses it", body = ApiError),
    ))]
async fn delete_image(
    State(AppState { machines, .. }): State<AppState>,
    Path((group, name)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    machines.delete_image(&ImageRef { group, name })?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/api/v1/images/{group}/{name}/resize", operation_id = "resizeImage", tag = "images",
    params(("group" = String, Path), ("name" = String, Path)),
    request_body = ResizeImageRequest,
    responses(
        (status = 204, description = "Resized"),
        (status = 409, description = "A running machine uses it", body = ApiError),
        (status = 422, description = "Would shrink, or it's an ISO", body = ApiError),
    ))]
async fn resize_image(
    State(AppState { machines, .. }): State<AppState>,
    Path((group, name)): Path<(String, String)>,
    Json(request): Json<ResizeImageRequest>,
) -> ApiResult<StatusCode> {
    machines
        .resize_image(&ImageRef { group, name }, request.size_gib)
        .await?;
    Ok(StatusCode::NO_CONTENT)
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
    State(AppState {
        machines, shutdown, ..
    }): State<AppState>,
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
