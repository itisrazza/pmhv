use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SetPasswordRequest {
    pub password: String,
    /// Required when changing an existing password; not needed during first-launch setup.
    #[serde(default)]
    pub current_password: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AuthStatus {
    pub authenticated: bool,
    /// Signed in with the temporary password: a password has to be set before anything else.
    pub setup_required: bool,
}

/// Issued on first launch and by `pmhv auth reset`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TemporaryPassword {
    pub password: String,
}
