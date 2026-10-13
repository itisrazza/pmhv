#[cfg(not(unix))]
compile_error!("pmhvd only supports unix hosts for now; Windows support is planned");

mod api;
mod config;
mod paths;
mod server;
mod ui;

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use pmhv_model::report::error_chain;
use tracing_subscriber::EnvFilter;
use utoipa::OpenApi;

use crate::config::{Config, ConfigError};
use crate::paths::{Paths, PathsError};
use crate::server::ServeError;

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
            server::run(&config, &paths).await?;
            Ok(())
        }
    }
}
