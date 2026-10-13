use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Body of every non-2xx API response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ApiError {
    /// Stable, machine-readable error code, e.g. `not_found`.
    pub code: String,
    /// Human-readable description of what went wrong.
    pub message: String,
}
