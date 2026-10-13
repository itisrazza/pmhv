use std::path::PathBuf;

use pmhv_model::paths::{APP_DIR, control_socket_in, default_runtime_dir};

#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("cannot determine the user data directory; set --data-dir or PMHV_DATA_DIR")]
    NoDataDir,
}

/// Where pmhvd keeps persistent state and runtime sockets.
#[derive(Debug, Clone)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub runtime_dir: PathBuf,
}

impl Paths {
    /// Resolves directories, preferring explicit overrides over platform defaults.
    ///
    /// Data: `~/Library/Application Support/pmhv` on macOS, `$XDG_DATA_HOME/pmhv` on Linux.
    /// Runtime: see [`default_runtime_dir`].
    pub fn resolve(
        data_dir: Option<PathBuf>,
        runtime_dir: Option<PathBuf>,
    ) -> Result<Self, PathsError> {
        let data_dir = match data_dir {
            Some(dir) => dir,
            None => dirs::data_dir().ok_or(PathsError::NoDataDir)?.join(APP_DIR),
        };
        let runtime_dir = runtime_dir.unwrap_or_else(default_runtime_dir);
        Ok(Self {
            data_dir,
            runtime_dir,
        })
    }

    pub fn config_file(&self) -> PathBuf {
        self.data_dir.join("config.toml")
    }

    pub fn control_socket(&self) -> PathBuf {
        control_socket_in(&self.runtime_dir)
    }
}
