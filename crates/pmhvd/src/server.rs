use std::io;
use std::net::SocketAddr;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

use axum::Router;
use tokio::net::{TcpListener, UnixListener, UnixStream};
use tokio::sync::watch;
use tracing::info;

use std::sync::Arc;

use crate::api::{self, AppState};
use crate::auth::Auth;
use crate::config::Config;
use crate::machines::{Context, MachineError, Machines};
use crate::paths::Paths;
use crate::ui;

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("failed to listen on {addr}")]
    BindTcp { addr: SocketAddr, source: io::Error },
    #[error("failed to create runtime directory {path}")]
    CreateRuntimeDir { path: PathBuf, source: io::Error },
    #[error("pmhvd is already running (control socket {path} is accepting connections)")]
    AlreadyRunning { path: PathBuf },
    #[error("cannot check existing control socket {path}")]
    ProbeSocket { path: PathBuf, source: io::Error },
    #[error("failed to remove stale control socket {path}")]
    RemoveStaleSocket { path: PathBuf, source: io::Error },
    #[error("failed to listen on control socket {path}")]
    BindSocket { path: PathBuf, source: io::Error },
    #[error("failed to restrict permissions on control socket {path}")]
    SocketPermissions { path: PathBuf, source: io::Error },
    #[error("HTTP server failed")]
    Http(#[source] io::Error),
    #[error("control socket server failed")]
    Control(#[source] io::Error),
    #[error("failed to install signal handler")]
    Signal(#[source] io::Error),
    #[error(transparent)]
    Machines(#[from] MachineError),
}

pub fn app(state: AppState) -> Router {
    api::router(state).fallback(ui::serve)
}

/// Serves the same app over TCP (browser) and the control socket (CLI) until a shutdown signal.
pub async fn run(
    config: &Config,
    paths: &Paths,
    context: Context,
    auth: Arc<Auth>,
) -> Result<(), ServeError> {
    // The control socket goes first so a second instance reports "already running" rather than a port clash.
    let socket_path = paths.control_socket();
    let control = bind_control_socket(&paths.runtime_dir, &socket_path).await?;
    let addr = config.http.listen;
    let tcp = match TcpListener::bind(addr).await {
        Ok(tcp) => tcp,
        Err(source) => {
            remove_socket(&socket_path);
            return Err(ServeError::BindTcp { addr, source });
        }
    };

    // Loaded only once this is the one pmhvd, since loading reattaches to running shims.
    let machines = match Machines::load(context).await {
        Ok(machines) => machines,
        Err(error) => {
            remove_socket(&socket_path);
            return Err(error.into());
        }
    };

    info!(listen = %addr, control_socket = %socket_path.display(), "pmhvd listening");

    let (stop_tx, stop_rx) = watch::channel(false);
    let state = AppState {
        machines,
        auth,
        local: false,
        shutdown: stop_rx.clone(),
    };
    let local_state = AppState {
        local: true,
        ..state.clone()
    };
    let mut http_stop = stop_rx.clone();
    let mut control_stop = stop_rx;

    let http = axum::serve(tcp, app(state.clone())).with_graceful_shutdown(async move {
        let _ = http_stop.wait_for(|stop| *stop).await;
    });
    let control = axum::serve(control, app(local_state)).with_graceful_shutdown(async move {
        let _ = control_stop.wait_for(|stop| *stop).await;
    });
    let signal = async move {
        shutdown_signal().await?;
        info!("shutting down");
        stop_tx.send_replace(true);
        Ok::<(), ServeError>(())
    };

    let result = tokio::try_join!(
        async { http.await.map_err(ServeError::Http) },
        async { control.await.map_err(ServeError::Control) },
        signal,
    );
    remove_socket(&socket_path);
    result.map(|_| ())
}

async fn bind_control_socket(runtime_dir: &Path, path: &Path) -> Result<UnixListener, ServeError> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(runtime_dir)
        .map_err(|source| ServeError::CreateRuntimeDir {
            path: runtime_dir.to_owned(),
            source,
        })?;

    match UnixStream::connect(path).await {
        Ok(_) => {
            return Err(ServeError::AlreadyRunning {
                path: path.to_owned(),
            });
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
            info!(path = %path.display(), "removing stale control socket");
            std::fs::remove_file(path).map_err(|source| ServeError::RemoveStaleSocket {
                path: path.to_owned(),
                source,
            })?;
        }
        Err(source) => {
            return Err(ServeError::ProbeSocket {
                path: path.to_owned(),
                source,
            });
        }
    }

    let listener = UnixListener::bind(path).map_err(|source| ServeError::BindSocket {
        path: path.to_owned(),
        source,
    })?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|source| {
        ServeError::SocketPermissions {
            path: path.to_owned(),
            source,
        }
    })?;
    Ok(listener)
}

fn remove_socket(path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        tracing::warn!(path = %path.display(), %error, "failed to remove control socket");
    }
}

async fn shutdown_signal() -> Result<(), ServeError> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate()).map_err(ServeError::Signal)?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result.map_err(ServeError::Signal),
        _ = terminate.recv() => Ok(()),
    }
}
