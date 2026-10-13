use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A fully resolved Virtualization.framework machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VzLaunch {
    pub name: String,
    pub cpu_cores: u16,
    pub memory_mib: u32,
    pub boot: VzBoot,
    pub disks: Vec<VzDisk>,
    pub nat: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VzBoot {
    /// Apple's UEFI firmware, for Linux and other EFI guests. The variable store is created on
    /// first boot.
    Efi { variable_store: PathBuf },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VzDiskKind {
    Virtio,
    Nvme,
    /// USB mass storage, which is how VZ attaches install media.
    Usb,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VzDisk {
    /// A raw image; Virtualization.framework can't read qcow2.
    pub path: PathBuf,
    pub kind: VzDiskKind,
    pub read_only: bool,
}
