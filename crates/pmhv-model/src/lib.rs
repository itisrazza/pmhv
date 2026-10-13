//! Types shared between the pmhv daemon, CLI and web UI (via OpenAPI).

mod error;
mod host;
pub mod paths;
pub mod report;

pub use error::ApiError;
pub use host::HostInfo;
