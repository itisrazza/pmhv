#[cfg(not(unix))]
compile_error!("pmhv only supports unix hosts for now; Windows support is planned");

mod client;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use pmhv_model::HostInfo;
use pmhv_model::paths::{control_socket_in, default_runtime_dir};
use pmhv_model::report::error_chain;

use crate::client::{Client, ClientError};

#[derive(Debug, Parser)]
#[command(version, about = "Command-line client for pmhvd")]
struct Cli {
    /// pmhvd's runtime directory, where its control socket lives.
    #[arg(long, env = "PMHV_RUNTIME_DIR", global = true)]
    runtime_dir: Option<PathBuf>,
    /// Print raw JSON instead of a table.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show facts about the host pmhvd is running on.
    Host,
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error("failed to render JSON")]
    Json(#[source] serde_json::Error),
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pmhv: {}", error_chain(&error));
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), CliError> {
    let runtime_dir = cli.runtime_dir.unwrap_or_else(default_runtime_dir);
    let client = Client::new(control_socket_in(&runtime_dir));

    match cli.command {
        Command::Host => {
            let host: HostInfo = client.get("/api/v1/host").await?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&host).map_err(CliError::Json)?
                );
            } else {
                print_table(&[
                    ("version", &host.version),
                    ("os", &host.os),
                    ("arch", &host.arch),
                ]);
            }
        }
    }
    Ok(())
}

fn print_table(rows: &[(&str, &str)]) {
    let width = rows.iter().map(|(key, _)| key.len()).max().unwrap_or(0);
    for (key, value) in rows {
        println!("{key:<width$}  {value}");
    }
}
