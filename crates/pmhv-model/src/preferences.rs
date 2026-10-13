use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::MachineId;

/// The web UI's settings, kept by pmhvd so they follow the user between browsers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct Preferences {
    /// Machines pinned to the sidebar, in order.
    #[schema(value_type = Vec<String>)]
    pub favourites: Vec<MachineId>,
}
