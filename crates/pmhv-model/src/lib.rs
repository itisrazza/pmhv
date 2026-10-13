//! Types shared between the pmhv daemon, CLI and web UI (via OpenAPI).

mod error;
mod event;
mod host;
mod image;
pub mod machine;
pub mod paths;
pub mod report;

pub use error::ApiError;
pub use event::Event;
pub use host::HostInfo;
pub use image::{ImageFormat, ImageGroup, ImageInfo};
pub use machine::{
    Arch, Backend, ConsoleKind, CreateMachineRequest, DiskBus, DiskMedia, DiskSpec, Firmware,
    GuestOs, ImageRef, Machine, MachineId, MachineSpec, MachineState, NetworkSpec, NewDisk,
    PowerAction, PowerRequest, Thumbnail, ThumbnailKind,
};
