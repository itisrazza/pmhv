use serde::{Deserialize, Serialize};
use ulid::Ulid;
use utoipa::ToSchema;

pub type MachineId = Ulid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Qemu,
    Vz,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Arch {
    Aarch64,
    #[serde(rename = "x86_64")]
    X86_64,
}

impl Arch {
    /// The architecture pmhv was built for, if it's one pmhv can run guests for.
    pub fn host() -> Option<Self> {
        match std::env::consts::ARCH {
            "aarch64" => Some(Self::Aarch64),
            "x86_64" => Some(Self::X86_64),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Aarch64 => "aarch64",
            Self::X86_64 => "x86_64",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuestOs {
    Linux,
    Macos,
    Windows,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Firmware {
    Uefi,
    /// Legacy BIOS (SeaBIOS). x86 only.
    Bios,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleKind {
    /// Serial console.
    Text,
    /// VNC framebuffer.
    Graphical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiskBus {
    Virtio,
    Nvme,
    Sata,
    Usb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiskMedia {
    Disk,
    Cdrom,
}

/// An image in one of pmhv's storage groups.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub struct ImageRef {
    pub group: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DiskSpec {
    pub image: ImageRef,
    pub bus: DiskBus,
    pub media: DiskMedia,
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NetworkSpec {
    /// No network interface.
    None,
    /// The backend's built-in NAT.
    Nat,
}

/// Everything needed to run a machine. Stored as `machine.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineSpec {
    pub name: String,
    pub guest_os: GuestOs,
    pub backend: Backend,
    pub arch: Arch,
    pub firmware: Firmware,
    pub cpu_cores: u16,
    pub memory_mib: u32,
    #[serde(default)]
    pub disks: Vec<DiskSpec>,
    pub network: NetworkSpec,
    pub console: ConsoleKind,
    #[serde(default)]
    pub tpm: bool,
    /// Host ports forwarded into the guest, through the built-in NAT. QEMU only.
    #[serde(default)]
    pub port_forwards: Vec<PortForward>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Tcp,
    Udp,
}

/// Connections to `host_port` on the host's loopback address reach `guest_port` in the guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PortForward {
    pub protocol: Protocol,
    pub host_port: u16,
    pub guest_port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MachineState {
    Stopped,
    Starting,
    Running,
    Paused,
    Stopping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Machine {
    #[schema(value_type = String)]
    pub id: MachineId,
    pub spec: MachineSpec,
    pub state: MachineState,
    /// True when the guest architecture differs from the host's, so it runs under emulation.
    pub emulated: bool,
    /// Why the machine last stopped, if it didn't stop cleanly.
    pub last_error: Option<String>,
    /// The most recent capture of the machine's screen, if there's been one.
    pub thumbnail: Option<Thumbnail>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThumbnailKind {
    /// A PNG of the framebuffer.
    Image,
    /// The tail of the serial console, as plain text.
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Thumbnail {
    pub kind: ThumbnailKind,
    /// When it was captured, in milliseconds since the Unix epoch. Changes with every capture.
    pub captured_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PowerAction {
    Start,
    /// Ask the guest to shut down (ACPI power button).
    Stop,
    /// Stop immediately, like pulling the plug.
    ForceStop,
    Reset,
    Pause,
    Resume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PowerRequest {
    pub action: PowerAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NewDisk {
    pub size_gib: u32,
    pub bus: DiskBus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreateMachineRequest {
    pub spec: MachineSpec,
    /// Create a blank boot disk in the default image group and attach it first.
    pub new_boot_disk: Option<NewDisk>,
}
