use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Qcow2,
    Raw,
    Iso,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ImageInfo {
    pub group: String,
    pub name: String,
    pub format: ImageFormat,
    /// Space used on the host.
    pub allocated_bytes: u64,
    /// Size the guest sees.
    pub virtual_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ImageGroup {
    pub name: String,
    pub images: Vec<ImageInfo>,
}
