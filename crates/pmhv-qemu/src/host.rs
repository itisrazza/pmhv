//! Finding QEMU on the host and picking an accelerator.

use std::path::{Path, PathBuf};

use pmhv_model::Arch;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error(
        "{binary} not found in {searched:?}; install QEMU (`brew install qemu`) or set qemu.bin_dir in config.toml"
    )]
    BinaryNotFound {
        binary: String,
        searched: Vec<PathBuf>,
    },
    #[error(
        "KVM is not available (/dev/kvm is missing or not accessible); add your user to the kvm group, or set the machine's architecture to emulate it"
    )]
    KvmUnavailable,
    #[error("failed to resolve QEMU binary {path}")]
    Resolve {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("hardware acceleration for {0} guests isn't supported on this host")]
    NoAccelerator(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accelerator {
    Hvf,
    Kvm,
    Tcg,
}

impl Accelerator {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hvf => "hvf",
            Self::Kvm => "kvm",
            Self::Tcg => "tcg",
        }
    }
}

/// A located `qemu-system-*` binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QemuBinary {
    pub path: PathBuf,
    /// Installation prefix (the binary's `bin/` parent), used to find bundled firmware.
    pub prefix: Option<PathBuf>,
}

pub fn system_binary_name(arch: Arch) -> String {
    format!("qemu-system-{}", arch.as_str())
}

/// Looks for `name` in `bin_dir` when given, otherwise on `PATH`.
pub fn find_binary(name: &str, bin_dir: Option<&Path>) -> Result<PathBuf, HostError> {
    let searched: Vec<PathBuf> = match bin_dir {
        Some(dir) => vec![dir.to_owned()],
        None => std::env::var_os("PATH")
            .map(|path| std::env::split_paths(&path).collect())
            .unwrap_or_default(),
    };
    searched
        .iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| HostError::BinaryNotFound {
            binary: name.to_owned(),
            searched,
        })
}

pub fn find_system_binary(arch: Arch, bin_dir: Option<&Path>) -> Result<QemuBinary, HostError> {
    let path = find_binary(&system_binary_name(arch), bin_dir)?;
    // Homebrew links bin/ into its prefix; resolve the link so share/ is found next to the real binary.
    let resolved = std::fs::canonicalize(&path).map_err(|source| HostError::Resolve {
        path: path.clone(),
        source,
    })?;
    let prefix = resolved.parent().and_then(Path::parent).map(Path::to_owned);
    Ok(QemuBinary { path, prefix })
}

/// The accelerator for a guest of `guest` arch: hardware when it matches the host, TCG otherwise.
pub fn accelerator(guest: Arch, host: Option<Arch>) -> Result<Accelerator, HostError> {
    if host != Some(guest) {
        return Ok(Accelerator::Tcg);
    }
    match std::env::consts::OS {
        "macos" => Ok(Accelerator::Hvf),
        "linux" if kvm_available() => Ok(Accelerator::Kvm),
        "linux" => Err(HostError::KvmUnavailable),
        _ => Err(HostError::NoAccelerator(guest.as_str())),
    }
}

fn kvm_available() -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .is_ok()
}
