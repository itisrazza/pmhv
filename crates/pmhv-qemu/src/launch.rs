//! A fully resolved QEMU invocation and the command line it turns into.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use pmhv_model::{Arch, DiskBus, DiskMedia, MachineSpec, NetworkSpec};
use serde::{Deserialize, Serialize};

use crate::host::Accelerator;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QemuLaunch {
    pub name: String,
    pub binary: PathBuf,
    pub arch: Arch,
    pub accelerator: Accelerator,
    pub cpu_cores: u16,
    pub memory_mib: u32,
    pub firmware: LaunchFirmware,
    pub disks: Vec<LaunchDisk>,
    pub network: NetworkSpec,
    pub graphical: bool,
    pub tpm: Option<TpmLaunch>,
    pub sockets: Sockets,
}

/// An `swtpm` process emulating a TPM 2.0, with its state kept alongside the machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TpmLaunch {
    pub swtpm: PathBuf,
    pub state_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LaunchFirmware {
    Uefi { code: PathBuf, vars: PathBuf },
    Bios,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskFormat {
    Qcow2,
    Raw,
}

impl DiskFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Qcow2 => "qcow2",
            Self::Raw => "raw",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchDisk {
    pub path: PathBuf,
    pub format: DiskFormat,
    pub bus: DiskBus,
    pub media: DiskMedia,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sockets {
    pub qmp: PathBuf,
    pub serial: PathBuf,
    pub vnc: PathBuf,
    pub tpm: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpecError {
    #[error("machine name can't be empty")]
    EmptyName,
    #[error("a machine needs at least one CPU core")]
    NoCores,
    #[error("a machine needs at least 64 MiB of memory")]
    TooLittleMemory,
    #[error("legacy BIOS firmware is only available for x86_64 machines")]
    BiosNeedsX86,
    #[error("NVMe can't hold CD-ROM media; use SATA, USB or VirtIO for disk {index}")]
    NvmeCdrom { index: usize },
    #[error("a SATA controller has 6 ports, but {count} SATA disks are attached")]
    TooManySata { count: usize },
}

/// Checks the parts of a spec QEMU can't run, before anything is resolved on the host.
pub fn validate(spec: &MachineSpec) -> Result<(), SpecError> {
    if spec.name.trim().is_empty() {
        return Err(SpecError::EmptyName);
    }
    if spec.cpu_cores == 0 {
        return Err(SpecError::NoCores);
    }
    if spec.memory_mib < 64 {
        return Err(SpecError::TooLittleMemory);
    }
    if spec.firmware == pmhv_model::Firmware::Bios && spec.arch != Arch::X86_64 {
        return Err(SpecError::BiosNeedsX86);
    }
    if let Some(index) = spec
        .disks
        .iter()
        .position(|disk| disk.bus == DiskBus::Nvme && disk.media == DiskMedia::Cdrom)
    {
        return Err(SpecError::NvmeCdrom { index });
    }
    let sata = spec
        .disks
        .iter()
        .filter(|disk| disk.bus == DiskBus::Sata)
        .count();
    if sata > SATA_PORTS {
        return Err(SpecError::TooManySata { count: sata });
    }
    Ok(())
}

const SATA_PORTS: usize = 6;

pub fn machine_type(arch: Arch) -> &'static str {
    match arch {
        Arch::Aarch64 => "virt",
        Arch::X86_64 => "q35",
    }
}

impl QemuLaunch {
    /// Arguments for `binary`. The guest starts paused (`-S`) so the shim can attach to the
    /// serial console before any output, then resume it over QMP.
    pub fn args(&self) -> Vec<OsString> {
        let mut args = Args::default();
        args.push(["-name", &format!("guest={},debug-threads=on", self.name)]);
        args.push(["-nodefaults", "-no-user-config", "-S"]);
        args.push(["-machine", machine_type(self.arch)]);
        args.push(["-accel", self.accelerator.as_str()]);
        args.push(["-cpu", self.cpu_model()]);
        args.push(["-smp", &self.cpu_cores.to_string()]);
        args.push(["-m", &format!("{}M", self.memory_mib)]);
        self.firmware_args(&mut args);
        args.push(["-qmp"]);
        args.push_path("unix:", &self.sockets.qmp, ",server=on,wait=off");
        args.push(["-chardev"]);
        args.push_path(
            "socket,id=serial0,path=",
            &self.sockets.serial,
            ",server=on,wait=off",
        );
        args.push(["-serial", "chardev:serial0"]);
        self.display_args(&mut args);
        self.tpm_args(&mut args);
        self.disk_args(&mut args);
        self.network_args(&mut args);
        args.0
    }

    fn cpu_model(&self) -> &'static str {
        match self.accelerator {
            Accelerator::Hvf | Accelerator::Kvm => "host",
            Accelerator::Tcg => "max",
        }
    }

    fn firmware_args(&self, args: &mut Args) {
        let LaunchFirmware::Uefi { code, vars } = &self.firmware else {
            return;
        };
        args.push(["-drive"]);
        args.push_path("if=pflash,format=raw,unit=0,readonly=on,file=", code, "");
        args.push(["-drive"]);
        args.push_path("if=pflash,format=raw,unit=1,file=", vars, "");
    }

    fn display_args(&self, args: &mut Args) {
        if !self.graphical {
            args.push(["-display", "none"]);
            return;
        }
        let gpu = match self.arch {
            Arch::Aarch64 => "virtio-gpu-pci",
            Arch::X86_64 => "VGA",
        };
        args.push(["-device", gpu]);
        args.push(["-device", "qemu-xhci,id=input-xhci"]);
        args.push(["-device", "usb-kbd,bus=input-xhci.0"]);
        args.push(["-device", "usb-tablet,bus=input-xhci.0"]);
        args.push(["-display", "none", "-vnc"]);
        args.push_path("unix:", &self.sockets.vnc, "");
    }

    fn tpm_args(&self, args: &mut Args) {
        if self.tpm.is_none() {
            return;
        }
        args.push(["-chardev"]);
        args.push_path("socket,id=tpm-chardev,path=", &self.sockets.tpm, "");
        args.push(["-tpmdev", "emulator,id=tpm0,chardev=tpm-chardev"]);
        let device = match self.arch {
            Arch::Aarch64 => "tpm-tis-device,tpmdev=tpm0",
            Arch::X86_64 => "tpm-tis,tpmdev=tpm0",
        };
        args.push(["-device", device]);
    }

    fn disk_args(&self, args: &mut Args) {
        let has_bus = |bus| self.disks.iter().any(|disk| disk.bus == bus);
        let has_virtio_cdrom = self
            .disks
            .iter()
            .any(|disk| disk.bus == DiskBus::Virtio && disk.media == DiskMedia::Cdrom);
        if has_virtio_cdrom {
            args.push(["-device", "virtio-scsi-pci,id=scsi0"]);
        }
        if has_bus(DiskBus::Sata) {
            args.push(["-device", "ich9-ahci,id=ahci0"]);
        }
        if has_bus(DiskBus::Usb) {
            args.push(["-device", "qemu-xhci,id=disk-xhci"]);
        }

        let mut sata_port = 0;
        for (index, disk) in self.disks.iter().enumerate() {
            let id = format!("disk{index}");
            let media = match disk.media {
                DiskMedia::Disk => "disk",
                DiskMedia::Cdrom => "cdrom",
            };
            let read_only = if disk.read_only || disk.media == DiskMedia::Cdrom {
                "on"
            } else {
                "off"
            };
            args.push(["-drive"]);
            args.push_path(
                "file=",
                &disk.path,
                &format!(
                    ",format={},if=none,id={id},media={media},readonly={read_only}",
                    disk.format.as_str()
                ),
            );
            let device = match (disk.bus, disk.media) {
                (DiskBus::Virtio, DiskMedia::Disk) => format!("virtio-blk-pci,drive={id}"),
                (DiskBus::Virtio, DiskMedia::Cdrom) => format!("scsi-cd,drive={id},bus=scsi0.0"),
                (DiskBus::Nvme, _) => format!("nvme,drive={id},serial={id}"),
                (DiskBus::Sata, media) => {
                    let kind = if media == DiskMedia::Cdrom {
                        "ide-cd"
                    } else {
                        "ide-hd"
                    };
                    let device = format!("{kind},drive={id},bus=ahci0.{sata_port}");
                    sata_port += 1;
                    device
                }
                (DiskBus::Usb, _) => format!("usb-storage,drive={id},bus=disk-xhci.0"),
            };
            args.push(["-device", &format!("{device},bootindex={index}")]);
        }
    }

    fn network_args(&self, args: &mut Args) {
        match self.network {
            NetworkSpec::None => args.push(["-nic", "none"]),
            NetworkSpec::Nat => args.push(["-nic", "user,model=virtio-net-pci"]),
        }
    }
}

#[derive(Default)]
struct Args(Vec<OsString>);

impl Args {
    fn push<const N: usize>(&mut self, items: [&str; N]) {
        self.0.extend(items.into_iter().map(OsString::from));
    }

    /// Pushes `before{path}after` as one argument, escaping commas in the path as QEMU's
    /// option syntax requires, without forcing the path through UTF-8.
    fn push_path(&mut self, before: &str, path: &Path, after: &str) {
        let mut arg = OsString::from(before);
        arg.push(escape_commas(path));
        arg.push(after);
        self.0.push(arg);
    }
}

fn escape_commas(path: &Path) -> OsString {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let escaped = path
        .as_os_str()
        .as_bytes()
        .iter()
        .flat_map(|&byte| {
            if byte == b',' {
                vec![b',', b',']
            } else {
                vec![byte]
            }
        })
        .collect();
    OsString::from_vec(escaped)
}
