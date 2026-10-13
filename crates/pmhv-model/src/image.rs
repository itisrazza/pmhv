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
    /// Machines with this image attached.
    pub used_by: Vec<ImageUser>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ImageUser {
    #[schema(value_type = String)]
    pub id: crate::MachineId,
    pub name: String,
}

/// A new, empty qcow2 image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreateImageRequest {
    pub group: String,
    /// File name, ending in `.qcow2`.
    pub name: String,
    pub size_gib: u32,
}

/// Grows an image. Shrinking isn't offered: it truncates whatever the guest stored at the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ResizeImageRequest {
    pub size_gib: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ImageGroup {
    pub name: String,
    pub images: Vec<ImageInfo>,
}
