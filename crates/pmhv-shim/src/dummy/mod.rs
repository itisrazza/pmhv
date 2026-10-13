//! A simulated machine for developing and testing pmhv without a hypervisor. It boots (prints a
//! fake boot log on serial), offers a login prompt and a tiny shell, animates a framebuffer for
//! graphical consoles, and responds to power actions like a real guest.

mod frame;
mod vnc;

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use pmhv_driver::{DummyLaunch, MachineRuntimeDir, ShimPower};
use pmhv_model::MachineState;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, watch};
use tracing::info;

use crate::machine::{ControlError, Machine, RunError};
use frame::Frame;

const TICK: Duration = Duration::from_millis(100);
/// How long a requested shutdown takes, so the UI shows Stopping.
const SHUTDOWN_TICKS: u32 = 10;

/// The shim's handle on a running dummy machine.
pub(crate) struct Control {
    actions: mpsc::Sender<ShimPower>,
    frame: watch::Receiver<Arc<Frame>>,
    graphical: bool,
}

impl Control {
    pub(crate) async fn power(&self, action: ShimPower) -> Result<(), ControlError> {
        self.actions
            .send(action)
            .await
            .map_err(|_| ControlError::NotReady)
    }

    pub(crate) fn screenshot(&self, path: &Path) -> Result<(), ControlError> {
        if !self.graphical {
            return Err(ControlError::NotGraphical);
        }
        self.frame
            .borrow()
            .write_png(path)
            .map_err(ControlError::DummyScreenshot)
    }
}

fn boot_log(name: &str) -> Vec<String> {
    [
        "pmhv dummy firmware",
        "Booting from virtual disk 0...",
        "",
        "[    0.000000] dummy kernel 1.0 (pmhv) starting",
        "[    0.120000] Memory: ok",
        "[    0.310000] virtio: console ready",
        "[    0.540000] Mounting root file system... done",
        "[  OK  ] Started pmhv dummy services.",
        "",
    ]
    .iter()
    .map(|line| format!("{line}\r\n"))
    .chain([format!("{name} login: ")])
    .collect()
}

const HELP: &str = "Commands: help, uname, echo <text>, poweroff, reboot\r\n";

/// What the fake shell does with a line of input.
enum Command {
    Print(String),
    PowerOff,
    Reboot,
}

fn interpret(line: &str, logged_in: bool) -> Command {
    if !logged_in {
        return Command::Print("Welcome to the pmhv dummy machine. Type `help`.\r\n".to_owned());
    }
    let line = line.trim();
    match line.split_once(' ').unwrap_or((line, "")) {
        ("", _) => Command::Print(String::new()),
        ("help", _) => Command::Print(HELP.to_owned()),
        ("uname", _) => Command::Print("pmhv-dummy\r\n".to_owned()),
        ("echo", text) => Command::Print(format!("{text}\r\n")),
        ("poweroff", _) => Command::PowerOff,
        ("reboot", _) => Command::Reboot,
        (other, _) => Command::Print(format!("{other}: command not found\r\n")),
    }
}

/// The simulated guest's mutable state, advanced one tick at a time.
struct Guest {
    name: String,
    output: VecDeque<String>,
    line: String,
    logged_in: bool,
    paused: bool,
    shutting_down: Option<u32>,
    step: u64,
}

impl Guest {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            output: boot_log(name).into(),
            line: String::new(),
            logged_in: false,
            paused: false,
            shutting_down: None,
            step: 0,
        }
    }

    fn reboot(&mut self) {
        let name = self.name.clone();
        *self = Self::new(&name);
        self.output.push_front("\r\nRebooting...\r\n".to_owned());
    }

    fn begin_shutdown(&mut self) {
        if self.shutting_down.is_none() {
            self.output
                .push_back("\r\nPower button pressed: shutting down.\r\n".to_owned());
            self.shutting_down = Some(SHUTDOWN_TICKS);
        }
    }

    /// The next chunk to print: one line of boot log per tick, so it scrolls like a real boot,
    /// but typed echoes and prompts all at once.
    fn next_output(&mut self) -> String {
        let mut text = String::new();
        while let Some(next) = self.output.pop_front() {
            let whole_line = next.ends_with('\n');
            text.push_str(&next);
            if whole_line {
                break;
            }
        }
        text
    }

    /// Counts down a requested shutdown once its message has been printed.
    fn shutdown_finished(&mut self) -> bool {
        if !self.output.is_empty() {
            return false;
        }
        match self.shutting_down.as_mut() {
            Some(remaining) => {
                *remaining = remaining.saturating_sub(1);
                *remaining == 0
            }
            None => false,
        }
    }

    /// Echoes typed bytes and runs complete lines.
    fn input(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            match byte {
                b'\r' | b'\n' => {
                    self.output.push_back("\r\n".to_owned());
                    let line = std::mem::take(&mut self.line);
                    match interpret(&line, self.logged_in) {
                        Command::Print(text) => self.output.push_back(text),
                        Command::PowerOff => self.begin_shutdown(),
                        Command::Reboot => {
                            self.reboot();
                            return;
                        }
                    }
                    self.logged_in = true;
                    if self.shutting_down.is_none() {
                        self.output.push_back("$ ".to_owned());
                    }
                }
                0x7f | 0x08 => {
                    if self.line.pop().is_some() {
                        self.output.push_back("\x08 \x08".to_owned());
                    }
                }
                byte if byte.is_ascii_graphic() || byte == b' ' => {
                    self.line.push(char::from(byte));
                    self.output.push_back(char::from(byte).to_string());
                }
                _ => {}
            }
        }
    }
}

impl Machine {
    pub(crate) async fn run_dummy(
        &self,
        launch: &DummyLaunch,
        dir: &MachineRuntimeDir,
    ) -> Result<Option<String>, RunError> {
        let (ours, mut serial) = UnixStream::pair().map_err(RunError::DummySetup)?;
        self.console.attach(ours);

        let (frame_tx, frame_rx) = watch::channel(Arc::new(Frame::render(0, false)));
        if launch.graphical {
            let listener = UnixListener::bind(dir.vnc_socket()).map_err(RunError::DummySetup)?;
            tokio::spawn(vnc::serve(listener, frame_rx.clone(), launch.name.clone()));
        }
        let (actions_tx, mut actions) = mpsc::channel(8);
        let control = Control {
            actions: actions_tx,
            frame: frame_rx,
            graphical: launch.graphical,
        };
        // `set` only fails if already set, which can't happen: this is the only place it's set.
        let _ = self.dummy.set(control);

        let guest = Arc::new(Mutex::new(Guest::new(&launch.name)));
        let lock = || guest.lock().unwrap_or_else(PoisonError::into_inner);
        self.set_state(MachineState::Running);
        info!("dummy machine running");

        let mut tick = tokio::time::interval(TICK);
        let mut buffer = vec![0; 1024];
        loop {
            tokio::select! {
                action = actions.recv() => {
                    let Some(action) = action else { return Ok(None) };
                    match action {
                        ShimPower::ForceStop => return Ok(None),
                        ShimPower::Stop => {
                            lock().begin_shutdown();
                            self.set_state(MachineState::Stopping);
                        }
                        ShimPower::Reset => {
                            lock().reboot();
                            self.set_state(MachineState::Running);
                        }
                        ShimPower::Pause => {
                            let step = {
                                let mut guest = lock();
                                guest.paused = true;
                                guest.step
                            };
                            if launch.graphical {
                                frame_tx.send_replace(Arc::new(Frame::render(step, true)));
                            }
                            self.set_state(MachineState::Paused);
                        }
                        ShimPower::Resume => {
                            lock().paused = false;
                            self.set_state(MachineState::Running);
                        }
                    }
                }
                read = serial.read(&mut buffer), if !lock().paused => {
                    let count = read.map_err(RunError::DummySetup)?;
                    if count == 0 {
                        return Ok(None);
                    }
                    lock().input(&buffer[..count]);
                }
                _ = tick.tick() => {
                    let (text, frame, finished) = {
                        let mut guest = lock();
                        if guest.paused {
                            continue;
                        }
                        guest.step += 1;
                        let text = guest.next_output();
                        let finished = guest.shutdown_finished();
                        let frame = launch.graphical.then(|| Frame::render(guest.step, false));
                        (text, frame, finished)
                    };
                    if !text.is_empty() {
                        serial.write_all(text.as_bytes()).await.map_err(RunError::DummySetup)?;
                    }
                    if let Some(frame) = frame {
                        frame_tx.send_replace(Arc::new(frame));
                    }
                    if finished {
                        serial.write_all(b"Powered off.\r\n").await.map_err(RunError::DummySetup)?;
                        return Ok(None);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(guest: &mut Guest, text: &str) -> String {
        guest.output.clear();
        guest.input(text.as_bytes());
        guest.output.drain(..).collect()
    }

    #[test]
    fn shell_runs_commands() {
        let mut guest = Guest::new("test");
        let cases = [
            (
                "anyone\r",
                "anyone\r\nWelcome to the pmhv dummy machine. Type `help`.\r\n$ ",
            ),
            ("uname\r", "uname\r\npmhv-dummy\r\n$ "),
            ("echo hi there\r", "echo hi there\r\nhi there\r\n$ "),
            ("nope\r", "nope\r\nnope: command not found\r\n$ "),
            ("ab\x7fc\r", "ab\x08 \x08c\r\nac: command not found\r\n$ "),
        ];
        for (input, expected) in cases {
            assert_eq!(typed(&mut guest, input), expected, "{input:?}");
        }
    }

    #[test]
    fn poweroff_starts_shutting_down() {
        let mut guest = Guest::new("test");
        typed(&mut guest, "root\r");
        let output = typed(&mut guest, "poweroff\r");
        assert!(output.contains("shutting down"), "{output}");
        assert_eq!(guest.shutting_down, Some(SHUTDOWN_TICKS));
    }
}
