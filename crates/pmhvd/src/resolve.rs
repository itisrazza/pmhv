//! Turns a machine spec into a concrete launch plan for the shim, checking the host has what
//! the machine needs.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use pmhv_driver::{Launch, LaunchFile, MachineRuntimeDir};
use pmhv_model::{Arch, Backend, ConsoleKind, Firmware, ImageFormat, MachineId, MachineSpec};
use pmhv_qemu::firmware::{self, FirmwareError};
use pmhv_qemu::host::{self, HostError};
use pmhv_qemu::launch::machine_type;
use pmhv_qemu::{
    DiskFormat, LaunchDisk, LaunchFirmware, QemuLaunch, Sockets, SpecError, TpmLaunch,
};

use crate::config::QemuConfig;
use crate::images::{ImageError, ImageStore};
use crate::store::MachineStore;

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error(transparent)]
    Spec(#[from] SpecError),
    #[error("the Virtualization.framework backend isn't implemented yet; use QEMU")]
    VzUnsupported,
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
        if spec.backend == Backend::Vz {
            return Err(ResolveError::VzUnsupported);
        }
        pmhv_qemu::launch::validate(spec)?;
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
            launch: Launch::Qemu(QemuLaunch {
                name: spec.name.clone(),
                binary: binary.path,
                arch: spec.arch,
                accelerator,
                cpu_cores: spec.cpu_cores,
                memory_mib: spec.memory_mib,
                firmware,
                disks,
                network: spec.network,
                graphical: spec.console == ConsoleKind::Graphical,
                tpm,
                sockets: Sockets {
                    qmp: dir.qmp_socket(),
                    serial: dir.serial_socket(),
                    vnc: dir.vnc_socket(),
                    tpm: dir.tpm_socket(),
                },
            }),
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
