use std::collections::BTreeMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const DEFAULT_PORT: u16 = 8640;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file {path}")]
    Read { path: PathBuf, source: io::Error },
    #[error("invalid config file {path}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub http: HttpConfig,
    pub qemu: QemuConfig,
    pub images: ImagesConfig,
    pub thumbnails: ThumbnailsConfig,
    pub dev: DevConfig,
    /// Path to `pmhv-shim`. Defaults to the one next to `pmhvd`.
    pub shim: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HttpConfig {
    pub listen: SocketAddr,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_PORT)),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct QemuConfig {
    /// Directory holding `qemu-system-*` and `qemu-img`. Defaults to searching `PATH`.
    pub bin_dir: Option<PathBuf>,
    /// Firmware descriptor directories. Defaults to QEMU's standard locations.
    pub firmware_dirs: Option<Vec<PathBuf>>,
    /// Path to `swtpm`. Defaults to searching `PATH`.
    pub swtpm: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImagesConfig {
    /// Extra storage groups by name. The `default` group is always `<data dir>/images/default`
    /// unless overridden here.
    pub groups: BTreeMap<String, PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThumbnailsConfig {
    /// Seconds between captures of a running machine's screen.
    pub interval_secs: u64,
}

impl Default for ThumbnailsConfig {
    fn default() -> Self {
        Self { interval_secs: 300 }
    }
}

/// Settings for developing pmhv itself.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DevConfig {
    /// Offer the dummy backend: simulated machines that need no hypervisor.
    pub dummy_backend: bool,
}

impl Config {
    /// Loads the config file, or the defaults when it doesn't exist yet.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().expect("temp dir");
        let config = Config::load(&dir.path().join("config.toml")).expect("defaults");
        assert_eq!(
            config.http.listen,
            SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_PORT))
        );
        assert_eq!(config.thumbnails.interval_secs, 300);
        assert!(config.images.groups.is_empty());
    }

    #[test]
    fn parses_overrides_and_rejects_typos() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config.toml");
        let cases = [
            ("[http]\nlisten = \"0.0.0.0:9000\"\n", true),
            ("[images.groups]\nexternal = \"/Volumes/VMs\"\n", true),
            ("[thumbnails]\ninterval_secs = 60\n", true),
            ("[dev]\ndummy_backend = true\n", true),
            ("[http]\nlisten_on = \"0.0.0.0:9000\"\n", false),
            ("[http]\nlisten = \"not an address\"\n", false),
        ];
        for (text, valid) in cases {
            std::fs::write(&path, text).expect("write config");
            assert_eq!(Config::load(&path).is_ok(), valid, "{text}");
        }
    }
}
