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
