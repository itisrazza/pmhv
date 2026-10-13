#[cfg(not(unix))]
compile_error!("pmhv only supports unix hosts for now; Windows support is planned");

mod client;
mod service;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use pmhv_model::paths::{control_socket_in, default_runtime_dir};
use pmhv_model::report::error_chain;
use pmhv_model::{
    CreateMachineRequest, DiskBus, HostInfo, ImageGroup, Machine, MachineSpec, NewDisk,
    PowerAction, PowerRequest, TemporaryPassword,
};
use serde::Serialize;

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
    /// List machines.
    #[command(alias = "ls")]
    List,
    /// Show one machine.
    Show {
        machine: String,
    },
    /// Create a machine from a spec file (the same TOML as machine.toml).
    Create {
        spec: PathBuf,
        /// Also create a blank boot disk of this many GiB in the default image group.
        #[arg(long)]
        disk_gib: Option<u32>,
        /// Bus for the new boot disk.
        #[arg(long, value_enum, default_value_t = Bus::Virtio)]
        disk_bus: Bus,
    },
    /// Delete a stopped machine. Its disk images are kept.
    Delete {
        machine: String,
    },
    Start {
        machine: String,
    },
    /// Ask the guest to shut down.
    Stop {
        machine: String,
    },
    /// Stop immediately, like pulling the plug.
    ForceStop {
        machine: String,
    },
    Reset {
        machine: String,
    },
    Pause {
        machine: String,
    },
    Resume {
        machine: String,
    },
    /// List storage groups and their images.
    Images,
    /// Manage the web UI's sign-in.
    #[command(subcommand)]
    Auth(AuthCommand),
    /// Run pmhvd in the background at login (launchd on macOS, systemd on Linux).
    #[command(subcommand)]
    Service(ServiceCommand),
}

#[derive(Debug, Subcommand)]
enum ServiceCommand {
    /// Install and start the service.
    Install {
        /// Path to pmhvd. Defaults to the one next to pmhv.
        #[arg(long)]
        pmhvd: Option<PathBuf>,
        /// Print the service definition instead of installing it.
        #[arg(long)]
        print: bool,
    },
    /// Stop and remove the service. Running machines keep running.
    Uninstall,
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    /// Forget the password, sign out every browser, and print a new temporary password.
    Reset,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Bus {
    Virtio,
    Nvme,
    Sata,
    Usb,
}

impl From<Bus> for DiskBus {
    fn from(bus: Bus) -> Self {
        match bus {
            Bus::Virtio => Self::Virtio,
            Bus::Nvme => Self::Nvme,
            Bus::Sata => Self::Sata,
            Bus::Usb => Self::Usb,
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error(transparent)]
    Service(#[from] service::ServiceError),
    #[error("failed to render JSON")]
    Json(#[source] serde_json::Error),
    #[error("failed to read spec file {path}")]
    ReadSpec {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid spec file {path}")]
    ParseSpec {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("no machine matches {0:?}")]
    NoMatch(String),
    #[error("{query:?} matches more than one machine: {}", .names.join(", "))]
    Ambiguous { query: String, names: Vec<String> },
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
    let output = Output { json: cli.json };

    match cli.command {
        Command::Host => {
            let host: HostInfo = client.get("/api/v1/host").await?;
            output.print(&host, || {
                print_table(&[
                    ("version", host.version.clone()),
                    ("os", host.os.clone()),
                    ("arch", host.arch.clone()),
                ]);
            })
        }
        Command::List => {
            let machines: Vec<Machine> = client.get("/api/v1/machines").await?;
            output.print(&machines, || print_machines(&machines))
        }
        Command::Show { machine } => {
            let machine = find(&client, &machine).await?;
            output.print(&machine, || print_machine(&machine))
        }
        Command::Create {
            spec,
            disk_gib,
            disk_bus,
        } => {
            let request = CreateMachineRequest {
                spec: read_spec(&spec)?,
                new_boot_disk: disk_gib.map(|size_gib| NewDisk {
                    size_gib,
                    bus: disk_bus.into(),
                }),
            };
            let machine: Machine = client.post("/api/v1/machines", &request).await?;
            output.print(&machine, || {
                println!("created {} ({})", machine.spec.name, machine.id);
            })
        }
        Command::Delete { machine } => {
            let machine = find(&client, &machine).await?;
            client
                .delete(&format!("/api/v1/machines/{}", machine.id))
                .await?;
            println!("deleted {}", machine.spec.name);
            Ok(())
        }
        Command::Start { machine } => power(&client, &output, &machine, PowerAction::Start).await,
        Command::Stop { machine } => power(&client, &output, &machine, PowerAction::Stop).await,
        Command::ForceStop { machine } => {
            power(&client, &output, &machine, PowerAction::ForceStop).await
        }
        Command::Reset { machine } => power(&client, &output, &machine, PowerAction::Reset).await,
        Command::Pause { machine } => power(&client, &output, &machine, PowerAction::Pause).await,
        Command::Resume { machine } => power(&client, &output, &machine, PowerAction::Resume).await,
        Command::Auth(AuthCommand::Reset) => {
            let temporary: TemporaryPassword = client
                .post("/api/v1/auth/reset", &serde_json::json!({}))
                .await?;
            output.print(&temporary, || {
                println!("Temporary password: {}", temporary.password);
                println!("Sign in with it in the web UI, then choose a new password.");
            })
        }
        Command::Service(ServiceCommand::Install { pmhvd, print }) => {
            let daemon = match pmhvd {
                Some(path) => path,
                None => service::default_daemon()?,
            };
            let spec = service::ServiceSpec::new(daemon)?;
            if print {
                let (path, contents) = service::definition(&spec)?;
                println!("# {}", path.display());
                print!("{contents}");
            } else {
                let path = service::install(&spec)?;
                println!("installed {} and started pmhvd", path.display());
                println!("log: {}", spec.log.display());
            }
            Ok(())
        }
        Command::Service(ServiceCommand::Uninstall) => {
            let path = service::uninstall()?;
            println!("stopped pmhvd and removed {}", path.display());
            Ok(())
        }
        Command::Images => {
            let groups: Vec<ImageGroup> = client.get("/api/v1/images").await?;
            output.print(&groups, || print_images(&groups))
        }
    }
}

struct Output {
    json: bool,
}

impl Output {
    fn print<T: Serialize>(&self, value: &T, table: impl FnOnce()) -> Result<(), CliError> {
        if self.json {
            println!(
                "{}",
                serde_json::to_string_pretty(value).map_err(CliError::Json)?
            );
        } else {
            table();
        }
        Ok(())
    }
}

async fn power(
    client: &Client,
    output: &Output,
    query: &str,
    action: PowerAction,
) -> Result<(), CliError> {
    let machine = find(client, query).await?;
    let machine: Machine = client
        .post(
            &format!("/api/v1/machines/{}/power", machine.id),
            &PowerRequest { action },
        )
        .await?;
    output.print(&machine, || {
        println!("{}: {}", machine.spec.name, state_label(&machine));
    })
}

/// Finds a machine by exact ID, exact name, or a unique case-insensitive name prefix.
async fn find(client: &Client, query: &str) -> Result<Machine, CliError> {
    let machines: Vec<Machine> = client.get("/api/v1/machines").await?;
    if let Some(machine) = machines
        .iter()
        .find(|machine| machine.id.to_string() == query || machine.spec.name == query)
    {
        return Ok(machine.clone());
    }
    let lower = query.to_lowercase();
    let matches: Vec<&Machine> = machines
        .iter()
        .filter(|machine| machine.spec.name.to_lowercase().starts_with(&lower))
        .collect();
    match matches.as_slice() {
        [machine] => Ok((*machine).clone()),
        [] => Err(CliError::NoMatch(query.to_owned())),
        _ => Err(CliError::Ambiguous {
            query: query.to_owned(),
            names: matches
                .iter()
                .map(|machine| machine.spec.name.clone())
                .collect(),
        }),
    }
}

fn read_spec(path: &PathBuf) -> Result<MachineSpec, CliError> {
    let text = std::fs::read_to_string(path).map_err(|source| CliError::ReadSpec {
        path: path.clone(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| CliError::ParseSpec {
        path: path.clone(),
        source,
    })
}

fn state_label(machine: &Machine) -> String {
    let state = format!("{:?}", machine.state).to_lowercase();
    match &machine.last_error {
        Some(error) => format!("{state} (last error: {error})"),
        None => state,
    }
}

fn print_machines(machines: &[Machine]) {
    let rows: Vec<[String; 4]> = machines
        .iter()
        .map(|machine| {
            [
                machine.id.to_string(),
                machine.spec.name.clone(),
                format!("{:?}", machine.state).to_lowercase(),
                format!(
                    "{}{}",
                    machine.spec.arch.as_str(),
                    if machine.emulated { " (emulated)" } else { "" }
                ),
            ]
        })
        .collect();
    print_columns(&["ID", "NAME", "STATE", "ARCH"], &rows);
}

fn print_machine(machine: &Machine) {
    let disks = machine
        .spec
        .disks
        .iter()
        .map(|disk| {
            format!(
                "{:?} {:?}: {}/{}",
                disk.bus, disk.media, disk.image.group, disk.image.name
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    print_table(&[
        ("id", machine.id.to_string()),
        ("name", machine.spec.name.clone()),
        ("state", state_label(machine)),
        ("arch", machine.spec.arch.as_str().to_owned()),
        ("cpu cores", machine.spec.cpu_cores.to_string()),
        ("memory", format!("{} MiB", machine.spec.memory_mib)),
        (
            "console",
            format!("{:?}", machine.spec.console).to_lowercase(),
        ),
        ("disks", disks),
    ]);
}

fn print_images(groups: &[ImageGroup]) {
    let rows: Vec<[String; 4]> = groups
        .iter()
        .flat_map(|group| &group.images)
        .map(|image| {
            [
                image.group.clone(),
                image.name.clone(),
                format!("{:?}", image.format).to_lowercase(),
                format!(
                    "{} / {}",
                    gib(image.allocated_bytes),
                    gib(image.virtual_bytes)
                ),
            ]
        })
        .collect();
    print_columns(&["GROUP", "NAME", "FORMAT", "SIZE"], &rows);
}

#[allow(clippy::cast_precision_loss)]
fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / f64::from(1u32 << 30))
}

fn print_table(rows: &[(&str, String)]) {
    let width = rows.iter().map(|(key, _)| key.len()).max().unwrap_or(0);
    for (key, value) in rows {
        println!("{key:<width$}  {value}");
    }
}

fn print_columns<const N: usize>(headers: &[&str; N], rows: &[[String; N]]) {
    let widths: Vec<usize> = (0..N)
        .map(|column| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .chain([headers[column].len()])
                .max()
                .unwrap_or(0)
        })
        .collect();
    let line = |cells: Vec<&str>| {
        let padded: Vec<String> = cells
            .iter()
            .zip(&widths)
            .map(|(cell, width)| format!("{cell:<width$}"))
            .collect();
        println!("{}", padded.join("  ").trim_end());
    };
    line(headers.to_vec());
    for row in rows {
        line(row.iter().map(String::as_str).collect());
    }
}
