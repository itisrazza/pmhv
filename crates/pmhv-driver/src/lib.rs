//! What pmhvd and pmhv-shim agree on: the launch file, the per-machine runtime directory,
//! and the control protocol spoken over the shim's socket.

mod protocol;
mod runtime;

pub use protocol::{
    MessageError, Request, Response, ShimPower, ShimStatus, read_message, write_message,
};
pub use runtime::MachineRuntimeDir;

use pmhv_model::MachineId;
use pmhv_qemu::QemuLaunch;
use serde::{Deserialize, Serialize};

/// Written by pmhvd as `launch.json` in the machine's runtime directory; read by the shim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchFile {
    pub machine_id: MachineId,
    pub launch: Launch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum Launch {
    Qemu(QemuLaunch),
}
