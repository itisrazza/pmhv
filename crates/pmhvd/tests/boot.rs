//! Boots UEFI firmware under QEMU through pmhvd, the shim and the CLI, restarting pmhvd mid-run.
//!
//! Needs QEMU with EDK2 firmware and a built workspace, so it's ignored by default:
//! `just test-integration`.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

struct Daemon {
    child: Child,
}

impl Drop for Env {
    /// Force-stops anything still running, so a failed test doesn't leave QEMU behind.
    fn drop(&mut self) {
        let Ok(machines) = std::fs::read_dir(self.runtime.path().join("m")) else {
            return;
        };
        for machine in machines.flatten() {
            if let Ok(mut control) = UnixStream::connect(machine.path().join("ctl.sock")) {
                let _ = control.write_all(b"{\"type\":\"power\",\"action\":\"force_stop\"}\n");
                let _ = control.read(&mut [0; 256]);
            }
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Env {
    data: tempfile::TempDir,
    runtime: tempfile::TempDir,
    bin: PathBuf,
}

impl Env {
    fn new() -> Self {
        let data = tempfile::tempdir().expect("data dir");
        std::fs::write(
            data.path().join("config.toml"),
            "[http]\nlisten = \"127.0.0.1:0\"\n",
        )
        .expect("config");
        // Socket paths have to stay short, so the runtime directory lives directly in /tmp.
        let runtime = tempfile::Builder::new()
            .prefix("pmhv-it")
            .tempdir_in("/tmp")
            .expect("runtime dir");
        let bin = PathBuf::from(env!("CARGO_BIN_EXE_pmhvd"))
            .parent()
            .expect("target dir")
            .to_owned();
        assert!(
            bin.join("pmhv-shim").is_file() && bin.join("pmhv").is_file(),
            "build the whole workspace first: cargo build --workspace"
        );
        Self { data, runtime, bin }
    }

    fn daemon(&self) -> Daemon {
        let child = Command::new(self.bin.join("pmhvd"))
            .env("PMHV_DATA_DIR", self.data.path())
            .env("PMHV_RUNTIME_DIR", self.runtime.path())
            .stdin(Stdio::null())
            .spawn()
            .expect("start pmhvd");
        // A killed pmhvd leaves its socket file behind, so wait until one is actually listening.
        wait_until("pmhvd's control socket", Duration::from_secs(10), || {
            UnixStream::connect(self.runtime.path().join("pmhvd.sock")).is_ok()
        });
        Daemon { child }
    }

    fn cli(&self, args: &[&str]) -> Output {
        let output = Command::new(self.bin.join("pmhv"))
            .env("PMHV_RUNTIME_DIR", self.runtime.path())
            .args(args)
            .output()
            .expect("run pmhv");
        assert!(
            output.status.success(),
            "pmhv {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn state(&self, machine: &str) -> String {
        let output = self.cli(&["show", machine, "--json"]);
        let machine: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
        machine["state"].as_str().expect("state").to_owned()
    }

    fn wait_for_state(&self, machine: &str, state: &str) {
        wait_until(
            &format!("{machine} to be {state}"),
            Duration::from_secs(30),
            || self.state(machine) == state,
        );
    }
}

fn wait_until(what: &str, timeout: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Reads the serial console until any of `needles` shows up. Firmware builds differ in what
/// they print (some distros ship OVMF without the UEFI shell), so callers pass a few.
fn wait_for_console(socket: &Path, needles: &[&str]) {
    let mut stream = UnixStream::connect(socket).expect("console socket");
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .expect("timeout");
    let deadline = Instant::now() + Duration::from_mins(1);
    let mut output = Vec::new();
    let mut buffer = [0; 4096];
    while !needles
        .iter()
        .any(|needle| String::from_utf8_lossy(&output).contains(needle))
    {
        assert!(
            Instant::now() < deadline,
            "none of {needles:?} appeared on the console; got:\n{}",
            String::from_utf8_lossy(&output)
        );
        match stream.read(&mut buffer) {
            Ok(0) => panic!("console closed"),
            Ok(count) => output.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("console read failed: {error}"),
        }
    }
}

#[test]
#[ignore = "needs QEMU and a built workspace; run with `just test-integration`"]
fn boots_uefi_and_survives_a_daemon_restart() {
    let env = Env::new();
    let arch = std::env::consts::ARCH;
    let spec = env.data.path().join("spec.toml");
    std::fs::write(
        &spec,
        format!(
            "name = \"Integration\"\nguest_os = \"other\"\nbackend = \"qemu\"\narch = \"{arch}\"\n\
             firmware = \"uefi\"\ncpu_cores = 1\nmemory_mib = 512\nnetwork = \"none\"\nconsole = \"text\"\n"
        ),
    )
    .expect("spec");

    let daemon = env.daemon();
    env.cli(&[
        "create",
        spec.to_str().expect("UTF-8 path"),
        "--disk-gib",
        "1",
    ]);
    env.cli(&["start", "Integration"]);
    env.wait_for_state("Integration", "running");

    let output = env.cli(&["show", "Integration", "--json"]);
    let machine: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    let id = machine["id"].as_str().expect("id");
    let console = env.runtime.path().join("m").join(id).join("con.sock");
    wait_for_console(&console, &["Shell>", "BdsDxe", "UEFI", "Boot"]);

    drop(daemon);
    let _daemon = env.daemon();
    assert_eq!(
        env.state("Integration"),
        "running",
        "machine survives a pmhvd restart"
    );

    env.cli(&["force-stop", "Integration"]);
    env.wait_for_state("Integration", "stopped");
    assert!(
        !env.runtime.path().join("m").join(id).exists(),
        "runtime directory is cleaned up"
    );
}
