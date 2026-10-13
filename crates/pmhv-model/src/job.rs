use serde::{Deserialize, Serialize};
use ulid::Ulid;
use utoipa::ToSchema;

use crate::ImageFormat;

pub type JobId = Ulid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Running,
    Succeeded,
    Failed,
}

/// Long-running work, such as converting an image. Progress arrives on the event stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Job {
    #[schema(value_type = String)]
    pub id: JobId,
    /// What's being done, for display.
    pub description: String,
    pub state: JobState,
    /// From 0 to 1.
    pub progress: f32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ConvertImageRequest {
    pub format: ImageFormat,
    /// Name of the new image, in the same group.
    pub name: String,
}
