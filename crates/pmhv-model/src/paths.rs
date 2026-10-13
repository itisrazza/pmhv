//! Locations both the daemon and its clients need to agree on.

use std::path::{Path, PathBuf};

pub const APP_DIR: &str = "pmhv";

/// `$XDG_RUNTIME_DIR/pmhv` where it exists, otherwise `$TMPDIR/pmhv`.
pub fn default_runtime_dir() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIR)
}

pub fn control_socket_in(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join("pmhvd.sock")
}
