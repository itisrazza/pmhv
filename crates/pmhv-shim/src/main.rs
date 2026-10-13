//! Supervises one running machine on behalf of pmhvd, so the machine outlives pmhvd restarts.
//!
//! Usage: `pmhv-shim <runtime dir>/m/<id>/launch.json`

#[cfg(not(unix))]
compile_error!("pmhv-shim only supports unix hosts for now; Windows support is planned");

mod console;
mod control;
mod machine;

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use pmhv_driver::{Launch, LaunchFile, MachineRuntimeDir, ShimStatus};
use pmhv_model::MachineState;
use pmhv_model::report::error_chain;
use tokio::net::UnixListener;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use crate::machine::Machine;

/// How long to wait for watchers to read the final status before exiting anyway.
const EXIT_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, thiserror::Error)]
enum ShimError {
    #[error("usage: pmhv-shim <launch.json>")]
    Usage,
    #[error("failed to read launch file {path}")]
    ReadLaunch { path: PathBuf, source: io::Error },
    #[error("invalid launch file {path}")]
    ParseLaunch {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("launch file {0} isn't inside a machine runtime directory")]
    NoRuntimeDir(PathBuf),
    #[error("failed to listen on {path}")]
    Bind { path: PathBuf, source: io::Error },
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            error!(error = error_chain(&error), "shim failed");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), ShimError> {
    let launch_path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or(ShimError::Usage)?;
    let launch_file = read_launch(&launch_path)?;
    let dir = MachineRuntimeDir::from_path(
        launch_path
            .parent()
            .ok_or_else(|| ShimError::NoRuntimeDir(launch_path.clone()))?
            .to_owned(),
    );
    let Launch::Qemu(launch) = launch_file.launch;
    info!(machine = %launch_file.machine_id, "shim starting");

    let machine = Machine::new(launch.graphical);
    let (control, console) = match bind(&dir.shim_socket())
        .and_then(|control| Ok((control, bind(&dir.console_socket())?)))
    {
        Ok(listeners) => listeners,
        Err(error) => {
            remove_sockets(&dir);
            return Err(error);
        }
    };
    tokio::spawn(control::serve(Arc::clone(&machine), control));
    tokio::spawn(Arc::clone(&machine.console).serve(console));

    let error = match machine.run(&launch, &dir).await {
        Ok(error) => error,
        Err(error) => Some(error_chain(&error)),
    };
    if let Some(error) = &error {
        error!(error, "machine stopped abnormally");
    }
    machine.status.send_modify(|status| {
        status.state = MachineState::Stopped;
        status.error = error;
        status.exited = true;
    });
    write_exit_file(&dir, &machine.status.borrow());
    wait_for_watchers(&machine).await;
    remove_sockets(&dir);
    info!("shim exiting");
    Ok(())
}

fn read_launch(path: &Path) -> Result<LaunchFile, ShimError> {
    let text = std::fs::read_to_string(path).map_err(|source| ShimError::ReadLaunch {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| ShimError::ParseLaunch {
        path: path.to_owned(),
        source,
    })
}

fn bind(path: &Path) -> Result<UnixListener, ShimError> {
    let bind_error = |source| ShimError::Bind {
        path: path.to_owned(),
        source,
    };
    // pmhvd only starts a shim for a machine that has none, so anything here is stale.
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(bind_error(error)),
    }
    let listener = UnixListener::bind(path).map_err(bind_error)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(bind_error)?;
    Ok(listener)
}

async fn wait_for_watchers(machine: &Machine) {
    let deadline = tokio::time::Instant::now() + EXIT_GRACE;
    while machine.status.receiver_count() > 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn write_exit_file(dir: &MachineRuntimeDir, status: &ShimStatus) {
    let path = dir.exit_file();
    let written = serde_json::to_vec(status)
        .map_err(io::Error::from)
        .and_then(|json| std::fs::write(&path, json));
    if let Err(error) = written {
        error!(path = %path.display(), %error, "failed to write exit file");
    }
}

fn remove_sockets(dir: &MachineRuntimeDir) {
    for path in [dir.shim_socket(), dir.console_socket()] {
        if let Err(error) = std::fs::remove_file(&path)
            && error.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!(path = %path.display(), %error, "failed to remove socket");
        }
    }
}
