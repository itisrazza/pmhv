use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Facts about the machine pmhvd is running on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HostInfo {
    /// pmhvd version.
    pub version: String,
    /// Host operating system, as Rust names it (`macos`, `linux`, `windows`).
    pub os: String,
    /// Host CPU architecture, as Rust names it (`aarch64`, `x86_64`).
    pub arch: String,
}
