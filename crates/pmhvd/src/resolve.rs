//! Turns a machine spec into a concrete launch plan for the shim, checking the host has what
//! the machine needs.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use pmhv_driver::{Launch, LaunchFile, MachineRuntimeDir};
use pmhv_model::{
    Arch, Backend, ConsoleKind, DiskBus, DiskMedia, Firmware, GuestOs, ImageFormat, MachineId,
    MachineSpec, NetworkSpec,
};
use pmhv_qemu::firmware::{self, FirmwareError};
use pmhv_qemu::host::{self, HostError};
use pmhv_qemu::launch::machine_type;
use pmhv_qemu::{
    DiskFormat, LaunchDisk, LaunchFirmware, QemuLaunch, Sockets, SpecError, TpmLaunch,
};
use pmhv_vz::{VzBoot, VzDisk, VzDiskKind, VzLaunch};

use crate::config::QemuConfig;
use crate::images::{ImageError, ImageStore};
use crate::store::MachineStore;

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error(transparent)]
    Spec(#[from] SpecError),
    #[error(transparent)]
    Vz(#[from] VzSpecError),
    #[error("pmhv can't run guests on a {0} host")]
    UnsupportedHost(&'static str),
    #[error(transparent)]
    Host(#[from] HostError),
    #[error(transparent)]
    Firmware(#[from] FirmwareError),
    #[error("disk {index}")]
    Disk {
        index: usize,
        #[source]
        source: ImageError,
    },
    #[error("failed to create firmware variable store {path}")]
    Nvram { path: PathBuf, source: io::Error },
}

pub struct Resolver<'a> {
    pub qemu: &'a QemuConfig,
    pub store: &'a MachineStore,
    pub images: &'a ImageStore,
}

impl Resolver<'_> {
    /// Checks a spec without touching the host's QEMU install, for create and update.
    pub fn check(&self, spec: &MachineSpec) -> Result<(), ResolveError> {
        match spec.backend {
            Backend::Qemu => pmhv_qemu::launch::validate(spec)?,
            Backend::Vz => Self::check_vz(spec)?,
        }
        for (index, disk) in spec.disks.iter().enumerate() {
            self.images
                .resolve(&disk.image)
                .map_err(|source| ResolveError::Disk { index, source })?;
        }
        Ok(())
    }

    pub fn resolve(
        &self,
        id: MachineId,
        spec: &MachineSpec,
        dir: &MachineRuntimeDir,
    ) -> Result<LaunchFile, ResolveError> {
        self.check(spec)?;
        if spec.backend == Backend::Vz {
            return Ok(LaunchFile {
                machine_id: id,
                launch: Launch::Vz(self.vz_launch(id, spec)?),
            });
        }
        let host_arch =
            Arch::host().ok_or(ResolveError::UnsupportedHost(std::env::consts::ARCH))?;
        let binary = host::find_system_binary(spec.arch, self.qemu.bin_dir.as_deref())?;
        let accelerator = host::accelerator(spec.arch, Some(host_arch))?;
        let firmware = match spec.firmware {
            Firmware::Bios => LaunchFirmware::Bios,
            Firmware::Uefi => self.uefi(id, spec.arch, binary.prefix.as_deref())?,
        };
        let disks = spec
            .disks
            .iter()
            .enumerate()
            .map(|(index, disk)| {
                let (path, format) = self
                    .images
                    .resolve(&disk.image)
                    .map_err(|source| ResolveError::Disk { index, source })?;
                Ok(LaunchDisk {
                    path,
                    format: match format {
                        ImageFormat::Qcow2 => DiskFormat::Qcow2,
                        ImageFormat::Raw | ImageFormat::Iso => DiskFormat::Raw,
                    },
                    bus: disk.bus,
                    media: disk.media,
                    read_only: disk.read_only,
                })
            })
            .collect::<Result<_, ResolveError>>()?;
        let tpm = if spec.tpm {
            Some(TpmLaunch {
                swtpm: match &self.qemu.swtpm {
                    Some(path) => path.clone(),
                    None => host::find_binary("swtpm", None)?,
                },
                state_dir: self.store.tpm_dir(id),
            })
        } else {
            None
        };

        Ok(LaunchFile {
            machine_id: id,
            launch: Launch::Qemu(Box::new(QemuLaunch {
                name: spec.name.clone(),
                binary: binary.path,
                arch: spec.arch,
                accelerator,
                cpu_cores: spec.cpu_cores,
                memory_mib: spec.memory_mib,
                firmware,
                disks,
                network: spec.network,
                port_forwards: spec.port_forwards.clone(),
                graphical: spec.console == ConsoleKind::Graphical,
                tpm,
                sockets: Sockets {
                    qmp: dir.qmp_socket(),
                    serial: dir.serial_socket(),
                    vnc: dir.vnc_socket(),
                    tpm: dir.tpm_socket(),
                },
            })),
        })
    }

    /// UEFI code plus this machine's own variable store, created from the template on first boot.
    fn uefi(
        &self,
        id: MachineId,
        arch: Arch,
        prefix: Option<&std::path::Path>,
    ) -> Result<LaunchFirmware, ResolveError> {
        let dirs = self
            .qemu
            .firmware_dirs
            .clone()
            .unwrap_or_else(|| firmware::search_dirs(prefix));
        let found = firmware::find_uefi(arch, machine_type(arch), &dirs)?;
        let vars = self.store.nvram_file(id);
        if !vars.exists() {
            // Copies keep the template's permissions, which are often read-only.
            std::fs::copy(&found.vars_template, &vars)
                .and_then(|_| {
                    std::fs::set_permissions(&vars, std::fs::Permissions::from_mode(0o600))
                })
                .map_err(|source| ResolveError::Nvram {
                    path: vars.clone(),
                    source,
                })?;
        }
        Ok(LaunchFirmware::Uefi {
            code: found.code,
            vars,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VzSpecError {
    #[error("Apple Virtualization is only available on macOS")]
    NotMacos,
    #[error("installing macOS guests from an IPSW isn't implemented yet")]
    MacosGuest,
    #[error(
        "graphical consoles on Apple Virtualization need a VNC server in the guest, which isn't wired up yet; use a text console"
    )]
    Graphical,
    #[error("Apple Virtualization has no TPM")]
    Tpm,
    #[error(
        "Apple Virtualization doesn't forward ports; its NAT gives the guest an address the host can reach directly"
    )]
    PortForwards,
    #[error("Apple Virtualization only boots UEFI")]
    Bios,
    #[error("Apple Virtualization can't emulate other architectures; use QEMU for {0} guests")]
    Emulated(&'static str),
    #[error("machine name can't be empty")]
    EmptyName,
    #[error("a machine needs at least one CPU core")]
    NoCores,
    #[error("Apple Virtualization needs at least 128 MiB of memory")]
    TooLittleMemory,
    #[error("disk {index} is qcow2, which Apple Virtualization can't read; use a raw .img image")]
    Qcow2 { index: usize },
    #[error("disk {index}: Apple Virtualization has no SATA; use VirtIO, NVMe or USB")]
    Sata { index: usize },
    #[error("disk {index}: NVMe can't hold CD-ROM media; use USB or VirtIO")]
    NvmeCdrom { index: usize },
}

impl Resolver<'_> {
    fn check_vz(spec: &MachineSpec) -> Result<(), VzSpecError> {
        if !cfg!(target_os = "macos") {
            return Err(VzSpecError::NotMacos);
        }
        if spec.guest_os == GuestOs::Macos {
            return Err(VzSpecError::MacosGuest);
        }
        if spec.console == ConsoleKind::Graphical {
            return Err(VzSpecError::Graphical);
        }
        if spec.tpm {
            return Err(VzSpecError::Tpm);
        }
        if !spec.port_forwards.is_empty() {
            return Err(VzSpecError::PortForwards);
        }
        if spec.firmware == Firmware::Bios {
            return Err(VzSpecError::Bios);
        }
        if Arch::host() != Some(spec.arch) {
            return Err(VzSpecError::Emulated(spec.arch.as_str()));
        }
        if spec.name.trim().is_empty() {
            return Err(VzSpecError::EmptyName);
        }
        if spec.cpu_cores == 0 {
            return Err(VzSpecError::NoCores);
        }
        if spec.memory_mib < 128 {
            return Err(VzSpecError::TooLittleMemory);
        }
        for (index, disk) in spec.disks.iter().enumerate() {
            if crate::images::format_of(&disk.image.name) == Some(ImageFormat::Qcow2) {
                return Err(VzSpecError::Qcow2 { index });
            }
            match (disk.bus, disk.media) {
                (DiskBus::Sata, _) => return Err(VzSpecError::Sata { index }),
                (DiskBus::Nvme, DiskMedia::Cdrom) => return Err(VzSpecError::NvmeCdrom { index }),
                _ => {}
            }
        }
        Ok(())
    }

    fn vz_launch(&self, id: MachineId, spec: &MachineSpec) -> Result<VzLaunch, ResolveError> {
        let disks = spec
            .disks
            .iter()
            .enumerate()
            .map(|(index, disk)| {
                let (path, _) = self
                    .images
                    .resolve(&disk.image)
                    .map_err(|source| ResolveError::Disk { index, source })?;
                let kind = match disk.bus {
                    DiskBus::Virtio => VzDiskKind::Virtio,
                    DiskBus::Nvme => VzDiskKind::Nvme,
                    DiskBus::Usb => VzDiskKind::Usb,
                    DiskBus::Sata => return Err(VzSpecError::Sata { index }.into()),
                };
                Ok(VzDisk {
                    path,
                    kind,
                    read_only: disk.read_only || disk.media == DiskMedia::Cdrom,
                })
            })
            .collect::<Result<_, ResolveError>>()?;
        Ok(VzLaunch {
            name: spec.name.clone(),
            cpu_cores: spec.cpu_cores,
            memory_mib: spec.memory_mib,
            boot: VzBoot::Efi {
                variable_store: self.store.machine_dir(id).join("efi-vars.fd"),
            },
            disks,
            nat: spec.network == NetworkSpec::Nat,
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use pmhv_model::{DiskSpec, ImageRef};

    use super::*;

    fn spec() -> MachineSpec {
        MachineSpec {
            name: "vz".to_owned(),
            guest_os: GuestOs::Linux,
            backend: Backend::Vz,
            arch: Arch::host().expect("supported host"),
            firmware: Firmware::Uefi,
            cpu_cores: 2,
            memory_mib: 1024,
            disks: Vec::new(),
            network: NetworkSpec::Nat,
            console: ConsoleKind::Text,
            tpm: false,
            port_forwards: Vec::new(),
            notes: String::new(),
        }
    }

    fn disk(name: &str, bus: DiskBus, media: DiskMedia) -> DiskSpec {
        DiskSpec {
            image: ImageRef {
                group: "default".to_owned(),
                name: name.to_owned(),
            },
            bus,
            media,
            read_only: false,
        }
    }

    #[test]
    fn checks_vz_specs() {
        type Case = (&'static str, fn(&mut MachineSpec), Option<&'static str>);
        let cases: [Case; 9] = [
            ("valid", |_| {}, None),
            (
                "macOS guest",
                |spec| spec.guest_os = GuestOs::Macos,
                Some("IPSW"),
            ),
            (
                "graphical",
                |spec| spec.console = ConsoleKind::Graphical,
                Some("VNC"),
            ),
            ("TPM", |spec| spec.tpm = true, Some("TPM")),
            ("BIOS", |spec| spec.firmware = Firmware::Bios, Some("UEFI")),
            (
                "qcow2 disk",
                |spec| spec.disks = vec![disk("a.qcow2", DiskBus::Virtio, DiskMedia::Disk)],
                Some("qcow2"),
            ),
            (
                "SATA disk",
                |spec| spec.disks = vec![disk("a.img", DiskBus::Sata, DiskMedia::Disk)],
                Some("SATA"),
            ),
            (
                "NVMe CD-ROM",
                |spec| spec.disks = vec![disk("a.iso", DiskBus::Nvme, DiskMedia::Cdrom)],
                Some("NVMe"),
            ),
            (
                "raw disk and USB installer",
                |spec| {
                    spec.disks = vec![
                        disk("a.img", DiskBus::Virtio, DiskMedia::Disk),
                        disk("b.iso", DiskBus::Usb, DiskMedia::Cdrom),
                    ];
                },
                None,
            ),
        ];
        for (name, change, expected) in cases {
            let mut spec = spec();
            change(&mut spec);
            let result = Resolver::check_vz(&spec);
            match expected {
                None => assert!(result.is_ok(), "{name}: {result:?}"),
                Some(fragment) => {
                    let message = result.expect_err(name).to_string();
                    assert!(message.contains(fragment), "{name}: {message}");
                }
            }
        }
    }
}
