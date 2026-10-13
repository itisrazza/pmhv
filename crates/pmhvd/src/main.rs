#[cfg(not(unix))]
compile_error!("pmhvd only supports unix hosts for now; Windows support is planned");

mod access;
mod api;
mod auth;
mod config;
mod console;
mod images;
mod machines;
mod paths;
mod preferences;
mod resolve;
mod server;
mod store;
mod thumbnails;
mod ui;

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use pmhv_model::report::error_chain;
use tracing_subscriber::EnvFilter;
use utoipa::OpenApi;

use std::sync::Arc;

use crate::auth::{Auth, AuthError};
use crate::config::{Config, ConfigError};
use crate::images::ImageStore;
use crate::machines::Context;
use crate::paths::{Paths, PathsError};
use crate::preferences::{PreferencesError, PreferencesStore};
use crate::server::ServeError;
use crate::store::MachineStore;

#[derive(Debug, Parser)]
#[command(version, about = "pmhv daemon")]
struct Cli {
    /// Directory for configuration, machines and images.
    #[arg(long, env = "PMHV_DATA_DIR", global = true)]
    data_dir: Option<PathBuf>,
    /// Directory for sockets and other runtime state.
    #[arg(long, env = "PMHV_RUNTIME_DIR", global = true)]
    runtime_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the daemon (the default).
    Serve,
    /// Print the OpenAPI document for the HTTP API.
    Openapi,
}

#[derive(Debug, thiserror::Error)]
enum DaemonError {
    #[error(transparent)]
    Paths(#[from] PathsError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Serve(#[from] ServeError),
    #[error("failed to render OpenAPI document")]
    Openapi(#[source] serde_json::Error),
    #[error(transparent)]
    Qemu(#[from] pmhv_qemu::host::HostError),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Preferences(#[from] PreferencesError),
    #[error(
        "can't find pmhvd's own location to look for pmhv-shim next to it; set `shim` in config.toml"
    )]
    CurrentExe(#[source] std::io::Error),
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pmhvd: {}", error_chain(&error));
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), DaemonError> {
    match cli.command.unwrap_or(Command::Serve) {
        Command::Openapi => {
            let document = api::ApiDoc::openapi()
                .to_pretty_json()
                .map_err(DaemonError::Openapi)?;
            println!("{document}");
            Ok(())
        }
        Command::Serve => {
            tracing_subscriber::fmt()
                .with_writer(std::io::stderr)
                .with_ansi(std::io::stderr().is_terminal())
                .with_env_filter(
                    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
                )
                .init();
            let paths = Paths::resolve(cli.data_dir, cli.runtime_dir)?;
            let config = Config::load(&paths.config_file())?;
            let context = context(&config, &paths)?;
            let auth = Arc::new(Auth::load(&paths.data_dir)?);
            let preferences = Arc::new(PreferencesStore::load(&paths.data_dir)?);
            server::run(&config, &paths, context, auth, preferences).await?;
            Ok(())
        }
    }
}

fn context(config: &Config, paths: &Paths) -> Result<Context, DaemonError> {
    let qemu_img = pmhv_qemu::host::find_binary("qemu-img", config.qemu.bin_dir.as_deref())?;
    let shim = match &config.shim {
        Some(path) => path.clone(),
        None => std::env::current_exe()
            .map_err(DaemonError::CurrentExe)?
            .with_file_name("pmhv-shim"),
    };
    Ok(Context {
        store: MachineStore::new(&paths.data_dir),
        images: ImageStore::new(&paths.data_dir, &config.images.groups, qemu_img),
        qemu: config.qemu.clone(),
        runtime_dir: paths.runtime_dir.clone(),
        shim,
        thumbnail_interval: std::time::Duration::from_secs(config.thumbnails.interval_secs),
    })
}
