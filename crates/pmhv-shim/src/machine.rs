//! Runs one QEMU process and tracks its state from QMP events.

use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use pmhv_driver::{MachineRuntimeDir, ShimPower, ShimStatus};
use pmhv_model::MachineState;
use pmhv_qemu::{QemuLaunch, Qmp, QmpError, TpmLaunch};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::sync::{Notify, OnceCell, watch};
use tracing::{info, warn};

use crate::console::Console;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const STDERR_TAIL_LINES: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("failed to start {program}")]
    Spawn { program: PathBuf, source: io::Error },
    #[error("failed to create TPM state directory {path}")]
    TpmState { path: PathBuf, source: io::Error },
    #[error("{program} exited during startup ({status}){}", stderr_suffix(.stderr))]
    ExitedEarly {
        program: PathBuf,
        status: ExitStatus,
        stderr: Vec<String>,
    },
    #[error("{program} didn't create {socket} within {}s", STARTUP_TIMEOUT.as_secs())]
    StartupTimeout { program: PathBuf, socket: PathBuf },
    #[error("waiting for QEMU failed")]
    Wait(#[source] io::Error),
    #[error("QMP command failed")]
    Qmp(#[from] QmpError),
}

#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("the machine isn't running yet")]
    NotReady,
    #[error("the machine isn't graphical, so there's no screen to capture")]
    NotGraphical,
    #[error(transparent)]
    Qmp(#[from] QmpError),
}

fn stderr_suffix(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        format!(": {}", lines.join(" / "))
    }
}

pub struct Machine {
    pub status: watch::Sender<ShimStatus>,
    pub console: Arc<Console>,
    graphical: bool,
    qmp: OnceCell<Qmp>,
    kill: Notify,
}

impl Machine {
    pub fn new(graphical: bool) -> Arc<Self> {
        let (status, _) = watch::channel(ShimStatus {
            state: MachineState::Starting,
            error: None,
            exited: false,
        });
        Arc::new(Self {
            status,
            console: Console::new(),
            graphical,
            qmp: OnceCell::new(),
            kill: Notify::new(),
        })
    }

    fn set_state(&self, state: MachineState) {
        self.status.send_modify(|status| status.state = state);
    }

    pub async fn power(&self, action: ShimPower) -> Result<(), ControlError> {
        let Some(qmp) = self.qmp.get() else {
            if action == ShimPower::ForceStop {
                self.kill.notify_one();
                return Ok(());
            }
            return Err(ControlError::NotReady);
        };
        let command = match action {
            ShimPower::Stop => "system_powerdown",
            ShimPower::ForceStop => "quit",
            ShimPower::Reset => "system_reset",
            ShimPower::Pause => "stop",
            ShimPower::Resume => "cont",
        };
        qmp.execute(command, None).await?;
        Ok(())
    }

    pub async fn screenshot(&self, path: &Path) -> Result<(), ControlError> {
        if !self.graphical {
            return Err(ControlError::NotGraphical);
        }
        let qmp = self.qmp.get().ok_or(ControlError::NotReady)?;
        qmp.execute(
            "screendump",
            Some(json!({ "filename": path, "format": "png" })),
        )
        .await?;
        Ok(())
    }

    /// Runs QEMU until it exits. `Ok` carries the error to report if it exited abnormally.
    pub async fn run(
        &self,
        launch: &QemuLaunch,
        dir: &MachineRuntimeDir,
    ) -> Result<Option<String>, RunError> {
        let _tpm = match &launch.tpm {
            Some(tpm) => Some(start_swtpm(tpm, &dir.tpm_socket()).await?),
            None => None,
        };

        let mut qemu = Command::new(&launch.binary);
        qemu.args(launch.args())
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = qemu.spawn().map_err(|source| RunError::Spawn {
            program: launch.binary.clone(),
            source,
        })?;
        let stderr = forward_stderr(&mut child);
        info!(pid = child.id(), "QEMU started");

        let (qmp, mut events) =
            match wait_for(&mut child, &launch.binary, &dir.qmp_socket(), &stderr).await? {
                Started::Ready(stream) => Qmp::from_stream(stream).await?,
                Started::Exited(error) => return Err(error),
            };
        match wait_for(&mut child, &launch.binary, &dir.serial_socket(), &stderr).await? {
            Started::Ready(serial) => self.console.attach(serial),
            Started::Exited(error) => return Err(error),
        }
        qmp.execute("cont", None).await?;
        // `set` only fails if already set, which can't happen: this is the only place it's set.
        let _ = self.qmp.set(qmp);
        self.set_state(MachineState::Running);
        info!("machine running");

        let mut events_open = true;
        loop {
            tokio::select! {
                status = child.wait() => {
                    let status = status.map_err(RunError::Wait)?;
                    info!(%status, "QEMU exited");
                    return Ok((!status.success()).then(|| {
                        format!("QEMU exited with {status}{}", stderr_suffix(&tail(&stderr)))
                    }));
                }
                event = events.recv(), if events_open => match event {
                    Some(event) => self.on_event(&event.name),
                    None => events_open = false,
                },
                () = self.kill.notified() => {
                    if let Err(error) = child.start_kill() {
                        warn!(%error, "killing QEMU failed");
                    }
                }
            }
        }
    }

    fn on_event(&self, name: &str) {
        match name {
            "STOP" => self.set_state(MachineState::Paused),
            "RESUME" => self.set_state(MachineState::Running),
            "SHUTDOWN" => self.set_state(MachineState::Stopping),
            _ => {}
        }
    }
}

enum Started {
    Ready(UnixStream),
    Exited(RunError),
}

/// Waits until `socket` accepts a connection, or the child exits first.
async fn wait_for(
    child: &mut Child,
    program: &Path,
    socket: &Path,
    stderr: &StderrTail,
) -> Result<Started, RunError> {
    let deadline = tokio::time::Instant::now() + STARTUP_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().map_err(RunError::Wait)? {
            // Give the stderr forwarder a moment to catch the error QEMU printed.
            tokio::time::sleep(POLL_INTERVAL).await;
            return Ok(Started::Exited(RunError::ExitedEarly {
                program: program.to_owned(),
                status,
                stderr: tail(stderr),
            }));
        }
        if let Ok(stream) = UnixStream::connect(socket).await {
            return Ok(Started::Ready(stream));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(RunError::StartupTimeout {
                program: program.to_owned(),
                socket: socket.to_owned(),
            });
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

type StderrTail = Arc<Mutex<VecDeque<String>>>;

/// Copies the child's stderr into the shim's log, keeping the last few lines for error messages.
fn forward_stderr(child: &mut Child) -> StderrTail {
    let tail_lines: StderrTail = Arc::default();
    let Some(stderr) = child.stderr.take() else {
        return tail_lines;
    };
    let lines_tail = Arc::clone(&tail_lines);
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            eprintln!("qemu: {line}");
            let mut tail = lines_tail.lock().unwrap_or_else(PoisonError::into_inner);
            tail.push_back(line);
            if tail.len() > STDERR_TAIL_LINES {
                tail.pop_front();
            }
        }
    });
    tail_lines
}

fn tail(stderr: &StderrTail) -> Vec<String> {
    stderr
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .cloned()
        .collect()
}

async fn start_swtpm(tpm: &TpmLaunch, socket: &Path) -> Result<Child, RunError> {
    std::fs::create_dir_all(&tpm.state_dir).map_err(|source| RunError::TpmState {
        path: tpm.state_dir.clone(),
        source,
    })?;
    let mut state = std::ffi::OsString::from("dir=");
    state.push(&tpm.state_dir);
    let mut ctrl = std::ffi::OsString::from("type=unixio,path=");
    ctrl.push(socket);
    let mut command = Command::new(&tpm.swtpm);
    command
        .args(["socket", "--tpm2", "--tpmstate"])
        .arg(state)
        .arg("--ctrl")
        .arg(ctrl)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|source| RunError::Spawn {
        program: tpm.swtpm.clone(),
        source,
    })?;
    let stderr = forward_stderr(&mut child);
    // QEMU connects to swtpm's socket itself; just wait until it exists so QEMU doesn't race it.
    match wait_for(&mut child, &tpm.swtpm, socket, &stderr).await? {
        Started::Ready(_probe) => Ok(child),
        Started::Exited(error) => Err(error),
    }
}
