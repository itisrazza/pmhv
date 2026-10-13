use std::path::{Path, PathBuf};

use pmhv_model::MachineId;

/// `<runtime dir>/m/<id>/`, holding a running machine's sockets.
///
/// Unix socket paths are limited to 104 bytes on macOS, and `$TMPDIR` there is already about
/// 50, so names here are kept short.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineRuntimeDir(PathBuf);

impl MachineRuntimeDir {
    pub fn new(runtime_dir: &Path, id: MachineId) -> Self {
        Self(Self::parent(runtime_dir).join(id.to_string()))
    }

    pub fn from_path(path: PathBuf) -> Self {
        Self(path)
    }

    /// The directory all machine runtime directories live in.
    pub fn parent(runtime_dir: &Path) -> PathBuf {
        runtime_dir.join("m")
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn launch_file(&self) -> PathBuf {
        self.0.join("launch.json")
    }

    /// The shim's final [`crate::ShimStatus`], written just before it exits, so pmhvd can read
    /// why a machine stopped even if it wasn't watching at the time.
    pub fn exit_file(&self) -> PathBuf {
        self.0.join("exit.json")
    }

    /// The shim's control socket.
    pub fn shim_socket(&self) -> PathBuf {
        self.0.join("ctl.sock")
    }

    /// Raw serial console stream served by the shim (scrollback first, then live).
    pub fn console_socket(&self) -> PathBuf {
        self.0.join("con.sock")
    }

    pub fn qmp_socket(&self) -> PathBuf {
        self.0.join("qmp.sock")
    }

    pub fn serial_socket(&self) -> PathBuf {
        self.0.join("ser.sock")
    }

    pub fn vnc_socket(&self) -> PathBuf {
        self.0.join("vnc.sock")
    }

    pub fn tpm_socket(&self) -> PathBuf {
        self.0.join("tpm.sock")
    }
}
