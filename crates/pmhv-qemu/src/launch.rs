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

#[cfg(test)]
mod tests {
    use super::*;
    use pmhv_model::{ConsoleKind, DiskSpec, Firmware, GuestOs, ImageRef};

    fn launch() -> QemuLaunch {
        QemuLaunch {
            name: "test".to_owned(),
            binary: PathBuf::from("/bin/qemu-system-aarch64"),
            arch: Arch::Aarch64,
            accelerator: Accelerator::Hvf,
            cpu_cores: 2,
            memory_mib: 1024,
            firmware: LaunchFirmware::Bios,
            disks: Vec::new(),
            network: NetworkSpec::None,
            graphical: false,
            tpm: None,
            sockets: Sockets {
                qmp: PathBuf::from("/run/qmp.sock"),
                serial: PathBuf::from("/run/ser.sock"),
                vnc: PathBuf::from("/run/vnc.sock"),
                tpm: PathBuf::from("/run/tpm.sock"),
            },
        }
    }

    fn disk(path: &str, bus: DiskBus, media: DiskMedia) -> LaunchDisk {
        LaunchDisk {
            path: PathBuf::from(path),
            format: DiskFormat::Qcow2,
            bus,
            media,
            read_only: false,
        }
    }

    fn args(launch: &QemuLaunch) -> Vec<String> {
        launch
            .args()
            .into_iter()
            .map(|arg| arg.into_string().expect("test args are UTF-8"))
            .collect()
    }

    /// True when `needle` appears in `haystack` as consecutive arguments.
    fn contains_run(haystack: &[String], needle: &[&str]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window.iter().zip(needle).all(|(a, b)| a == b))
    }

    #[test]
    #[allow(clippy::too_many_lines, reason = "one table of cases")]
    fn builds_expected_arguments() {
        struct Case {
            name: &'static str,
            change: fn(&mut QemuLaunch),
            present: &'static [&'static [&'static str]],
            absent: &'static [&'static str],
        }
        let cases = [
            Case {
                name: "starts paused with QMP and serial sockets",
                change: |_| {},
                present: &[
                    &["-S"],
                    &["-qmp", "unix:/run/qmp.sock,server=on,wait=off"],
                    &[
                        "-chardev",
                        "socket,id=serial0,path=/run/ser.sock,server=on,wait=off",
                    ],
                    &["-serial", "chardev:serial0"],
                    &["-display", "none"],
                    &["-nic", "none"],
                ],
                absent: &["-vnc", "-tpmdev"],
            },
            Case {
                name: "hardware acceleration uses the host CPU",
                change: |_| {},
                present: &[&["-accel", "hvf", "-cpu", "host"], &["-machine", "virt"]],
                absent: &[],
            },
            Case {
                name: "emulation uses TCG's max CPU",
                change: |launch| {
                    launch.arch = Arch::X86_64;
                    launch.accelerator = Accelerator::Tcg;
                },
                present: &[&["-accel", "tcg", "-cpu", "max"], &["-machine", "q35"]],
                absent: &[],
            },
            Case {
                name: "UEFI maps code read-only and vars writable",
                change: |launch| {
                    launch.firmware = LaunchFirmware::Uefi {
                        code: PathBuf::from("/fw/code.fd"),
                        vars: PathBuf::from("/vm/nvram.fd"),
                    };
                },
                present: &[
                    &[
                        "-drive",
                        "if=pflash,format=raw,unit=0,readonly=on,file=/fw/code.fd",
                    ],
                    &["-drive", "if=pflash,format=raw,unit=1,file=/vm/nvram.fd"],
                ],
                absent: &[],
            },
            Case {
                name: "graphical ARM gets virtio-gpu, USB input and VNC",
                change: |launch| launch.graphical = true,
                present: &[
                    &["-device", "virtio-gpu-pci"],
                    &["-device", "usb-tablet,bus=input-xhci.0"],
                    &["-vnc", "unix:/run/vnc.sock"],
                ],
                absent: &[],
            },
            Case {
                name: "graphical x86 gets standard VGA",
                change: |launch| {
                    launch.arch = Arch::X86_64;
                    launch.graphical = true;
                },
                present: &[&["-device", "VGA"]],
                absent: &["virtio-gpu-pci"],
            },
            Case {
                name: "TPM on ARM uses the sysbus device",
                change: |launch| {
                    launch.tpm = Some(TpmLaunch {
                        swtpm: PathBuf::from("/bin/swtpm"),
                        state_dir: PathBuf::from("/vm/tpm"),
                    });
                },
                present: &[
                    &["-chardev", "socket,id=tpm-chardev,path=/run/tpm.sock"],
                    &["-device", "tpm-tis-device,tpmdev=tpm0"],
                ],
                absent: &[],
            },
            Case {
                name: "NAT uses user-mode networking",
                change: |launch| launch.network = NetworkSpec::Nat,
                present: &[&["-nic", "user,model=virtio-net-pci"]],
                absent: &[],
            },
            Case {
                name: "disks get controllers, devices and boot order",
                change: |launch| {
                    launch.disks = vec![
                        disk("/img/boot.qcow2", DiskBus::Virtio, DiskMedia::Disk),
                        disk("/img/setup.iso", DiskBus::Sata, DiskMedia::Cdrom),
                        disk("/img/data.qcow2", DiskBus::Sata, DiskMedia::Disk),
                        disk("/img/cd.iso", DiskBus::Virtio, DiskMedia::Cdrom),
                        disk("/img/fast.qcow2", DiskBus::Nvme, DiskMedia::Disk),
                        disk("/img/stick.img", DiskBus::Usb, DiskMedia::Disk),
                    ];
                },
                present: &[
                    &["-device", "virtio-scsi-pci,id=scsi0"],
                    &["-device", "ich9-ahci,id=ahci0"],
                    &["-device", "qemu-xhci,id=disk-xhci"],
                    &["-device", "virtio-blk-pci,drive=disk0,bootindex=0"],
                    &["-device", "ide-cd,drive=disk1,bus=ahci0.0,bootindex=1"],
                    &["-device", "ide-hd,drive=disk2,bus=ahci0.1,bootindex=2"],
                    &["-device", "scsi-cd,drive=disk3,bus=scsi0.0,bootindex=3"],
                    &["-device", "nvme,drive=disk4,serial=disk4,bootindex=4"],
                    &[
                        "-device",
                        "usb-storage,drive=disk5,bus=disk-xhci.0,bootindex=5",
                    ],
                    &[
                        "-drive",
                        "file=/img/setup.iso,format=qcow2,if=none,id=disk1,media=cdrom,readonly=on",
                    ],
                ],
                absent: &[],
            },
            Case {
                name: "commas in paths are doubled",
                change: |launch| {
                    launch.disks = vec![disk("/img/a,b.qcow2", DiskBus::Virtio, DiskMedia::Disk)];
                },
                present: &[&[
                    "-drive",
                    "file=/img/a,,b.qcow2,format=qcow2,if=none,id=disk0,media=disk,readonly=off",
                ]],
                absent: &[],
            },
        ];

        for case in cases {
            let mut launch = launch();
            (case.change)(&mut launch);
            let args = args(&launch);
            for run in case.present {
                assert!(
                    contains_run(&args, run),
                    "{}: missing {run:?} in {args:?}",
                    case.name
                );
            }
            for arg in case.absent {
                assert!(
                    !args.iter().any(|candidate| candidate.contains(arg)),
                    "{}: unexpected {arg} in {args:?}",
                    case.name
                );
            }
        }
    }

    fn spec() -> MachineSpec {
        MachineSpec {
            name: "test".to_owned(),
            guest_os: GuestOs::Linux,
            backend: pmhv_model::Backend::Qemu,
            arch: Arch::Aarch64,
            firmware: Firmware::Uefi,
            cpu_cores: 2,
            memory_mib: 1024,
            disks: Vec::new(),
            network: NetworkSpec::Nat,
            console: ConsoleKind::Text,
            tpm: false,
            notes: String::new(),
        }
    }

    fn spec_disk(bus: DiskBus, media: DiskMedia) -> DiskSpec {
        DiskSpec {
            image: ImageRef {
                group: "default".to_owned(),
                name: "disk.qcow2".to_owned(),
            },
            bus,
            media,
            read_only: false,
        }
    }

    #[test]
    fn validates_specs() {
        type Case = (&'static str, fn(&mut MachineSpec), Result<(), SpecError>);
        let cases: [Case; 7] = [
            ("valid", |_| {}, Ok(())),
            (
                "blank name",
                |spec| spec.name = "  ".to_owned(),
                Err(SpecError::EmptyName),
            ),
            (
                "no cores",
                |spec| spec.cpu_cores = 0,
                Err(SpecError::NoCores),
            ),
            (
                "tiny memory",
                |spec| spec.memory_mib = 32,
                Err(SpecError::TooLittleMemory),
            ),
            (
                "BIOS on ARM",
                |spec| spec.firmware = Firmware::Bios,
                Err(SpecError::BiosNeedsX86),
            ),
            (
                "NVMe CD-ROM",
                |spec| {
                    spec.disks = vec![
                        spec_disk(DiskBus::Virtio, DiskMedia::Disk),
                        spec_disk(DiskBus::Nvme, DiskMedia::Cdrom),
                    ];
                },
                Err(SpecError::NvmeCdrom { index: 1 }),
            ),
            (
                "seven SATA disks",
                |spec| spec.disks = vec![spec_disk(DiskBus::Sata, DiskMedia::Disk); 7],
                Err(SpecError::TooManySata { count: 7 }),
            ),
        ];
        for (name, change, expected) in cases {
            let mut spec = spec();
            change(&mut spec);
            assert_eq!(validate(&spec), expected, "{name}");
        }
    }
}
