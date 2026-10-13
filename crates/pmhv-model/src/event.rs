use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::machine::{Machine, MachineId};

/// Pushed to clients on the event stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    MachineUpdated {
        machine: Machine,
    },
    MachineRemoved {
        #[schema(value_type = String)]
        id: MachineId,
    },
}
