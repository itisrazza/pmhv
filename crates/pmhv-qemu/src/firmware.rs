//! UEFI firmware discovery using QEMU's firmware descriptor files
//! (`docs/interop/firmware.json` in the QEMU source).

use std::io;
use std::path::{Path, PathBuf};

use pmhv_model::Arch;
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum FirmwareError {
    #[error("failed to read firmware descriptor directory {path}")]
    ReadDir { path: PathBuf, source: io::Error },
    #[error("failed to read firmware descriptor {path}")]
    Read { path: PathBuf, source: io::Error },
    #[error("invalid firmware descriptor {path}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error(
        "no UEFI firmware for {arch} ({machine}) found in {searched:?}; install QEMU's EDK2 firmware or set qemu.firmware_dirs in config.toml"
    )]
    NotFound {
        arch: &'static str,
        machine: String,
        searched: Vec<PathBuf>,
    },
}

/// Firmware code plus the template its per-machine variable store is copied from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UefiFirmware {
    pub code: PathBuf,
    pub vars_template: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Descriptor {
    interface_types: Vec<String>,
    mapping: Mapping,
    targets: Vec<Target>,
    #[serde(default)]
    features: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "device", rename_all = "kebab-case")]
enum Mapping {
    Flash {
        executable: FlashFile,
        #[serde(rename = "nvram-template")]
        nvram_template: Option<FlashFile>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct FlashFile {
    filename: PathBuf,
    format: String,
}

#[derive(Debug, Deserialize)]
struct Target {
    architecture: String,
    machines: Vec<String>,
}

/// Descriptor directories, in QEMU's precedence order, for a QEMU installed under `prefix`.
pub fn search_dirs(prefix: Option<&Path>) -> Vec<PathBuf> {
    let installed = prefix.map(|prefix| prefix.join("share/qemu/firmware"));
    installed
        .into_iter()
        .chain(["/etc/qemu/firmware", "/usr/share/qemu/firmware"].map(PathBuf::from))
        .collect()
}

/// Finds non-Secure-Boot UEFI firmware with flash mapping for `arch` and `machine`.
pub fn find_uefi(
    arch: Arch,
    machine: &str,
    dirs: &[PathBuf],
) -> Result<UefiFirmware, FirmwareError> {
    let mut paths = Vec::new();
    for dir in dirs {
        paths.extend(descriptor_paths(dir)?);
    }
    // QEMU orders descriptors by file name across all directories.
    paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    for path in paths {
        let descriptor = read_descriptor(&path)?;
        if let Some(firmware) = matching_firmware(&descriptor, arch, machine) {
            return Ok(firmware);
        }
    }
    Err(FirmwareError::NotFound {
        arch: arch.as_str(),
        machine: machine.to_owned(),
        searched: dirs.to_vec(),
    })
}

fn descriptor_paths(dir: &Path) -> Result<Vec<PathBuf>, FirmwareError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(FirmwareError::ReadDir {
                path: dir.to_owned(),
                source,
            });
        }
    };
    let paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| FirmwareError::ReadDir {
            path: dir.to_owned(),
            source,
        })?;
    Ok(paths
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect())
}

fn read_descriptor(path: &Path) -> Result<Descriptor, FirmwareError> {
    let text = std::fs::read_to_string(path).map_err(|source| FirmwareError::Read {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| FirmwareError::Parse {
        path: path.to_owned(),
        source,
    })
}

fn matching_firmware(descriptor: &Descriptor, arch: Arch, machine: &str) -> Option<UefiFirmware> {
    let is_uefi = descriptor.interface_types.iter().any(|kind| kind == "uefi");
    let is_secure = descriptor
        .features
        .iter()
        .any(|feature| feature == "secure-boot" || feature == "requires-smm");
    let targets_machine = descriptor.targets.iter().any(|target| {
        target.architecture == arch.as_str()
            && target
                .machines
                .iter()
                .any(|pattern| machine_matches(pattern, machine))
    });
    if !is_uefi || is_secure || !targets_machine {
        return None;
    }
    match &descriptor.mapping {
        Mapping::Flash {
            executable,
            nvram_template: Some(template),
        } if executable.format == "raw" && template.format == "raw" => Some(UefiFirmware {
            code: executable.filename.clone(),
            vars_template: template.filename.clone(),
        }),
        _ => None,
    }
}

/// Descriptor patterns are globs over versioned machine names (`virt-*`, `pc-q35-*`);
/// pmhv uses the unversioned aliases (`virt`, `q35`), which QEMU resolves to the latest version.
fn machine_matches(pattern: &str, machine: &str) -> bool {
    let Some(prefix) = pattern.strip_suffix("-*") else {
        return pattern == machine;
    };
    let alias = prefix.strip_prefix("pc-").unwrap_or(prefix);
    machine == alias || machine.starts_with(&format!("{prefix}-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_machine_patterns() {
        let cases = [
            ("virt-*", "virt", true),
            ("virt-*", "virt-9.2", true),
            ("pc-q35-*", "q35", true),
            ("pc-q35-*", "pc-q35-10.0", true),
            ("pc-q35-*", "pc-i440fx-10.0", false),
            ("pc-i440fx-*", "q35", false),
            ("virt", "virt", true),
            ("virt-*", "virtual", false),
        ];
        for (pattern, machine, expected) in cases {
            assert_eq!(
                machine_matches(pattern, machine),
                expected,
                "{pattern} vs {machine}"
            );
        }
    }

    fn descriptor(arch: &str, machine: &str, features: &[&str], code: &str) -> String {
        serde_json::json!({
            "description": "test",
            "interface-types": ["uefi"],
            "mapping": {
                "device": "flash",
                "executable": { "filename": code, "format": "raw" },
                "nvram-template": { "filename": format!("{code}.vars"), "format": "raw" },
            },
            "targets": [{ "architecture": arch, "machines": [machine] }],
            "features": features,
            "tags": [],
        })
        .to_string()
    }

    #[test]
    fn finds_first_matching_non_secure_firmware() {
        let dir = tempfile::tempdir().expect("temp dir");
        let files = [
            (
                "10-secure.json",
                descriptor(
                    "x86_64",
                    "pc-q35-*",
                    &["secure-boot", "requires-smm"],
                    "/fw/secure",
                ),
            ),
            (
                "20-arm.json",
                descriptor("aarch64", "virt-*", &[], "/fw/arm"),
            ),
            (
                "30-x86.json",
                descriptor("x86_64", "pc-q35-*", &["acpi-s3"], "/fw/x86"),
            ),
            (
                "40-x86-later.json",
                descriptor("x86_64", "pc-q35-*", &[], "/fw/x86-later"),
            ),
            ("notes.txt", "not a descriptor".to_owned()),
        ];
        for (name, contents) in files {
            std::fs::write(dir.path().join(name), contents).expect("write descriptor");
        }
        let dirs = [dir.path().to_owned(), PathBuf::from("/nonexistent")];

        let x86 = find_uefi(Arch::X86_64, "q35", &dirs).expect("x86 firmware");
        assert_eq!(x86.code, PathBuf::from("/fw/x86"));
        assert_eq!(x86.vars_template, PathBuf::from("/fw/x86.vars"));
        let arm = find_uefi(Arch::Aarch64, "virt", &dirs).expect("ARM firmware");
        assert_eq!(arm.code, PathBuf::from("/fw/arm"));
    }

    #[test]
    fn reports_missing_firmware() {
        let dir = tempfile::tempdir().expect("temp dir");
        let error = find_uefi(Arch::Aarch64, "virt", &[dir.path().to_owned()])
            .expect_err("nothing to find");
        assert!(matches!(error, FirmwareError::NotFound { .. }), "{error:?}");
    }
}
