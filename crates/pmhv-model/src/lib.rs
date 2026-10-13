//! Types shared between the pmhv daemon, CLI and web UI (via OpenAPI).

mod auth;
mod error;
mod event;
mod host;
mod image;
mod job;
pub mod machine;
pub mod paths;
mod preferences;
pub mod report;

pub use auth::{AuthStatus, LoginRequest, SetPasswordRequest, TemporaryPassword};
pub use error::ApiError;
pub use event::Event;
pub use host::HostInfo;
pub use image::{
    CreateImageRequest, ImageFormat, ImageGroup, ImageInfo, ImageUser, ResizeImageRequest,
};
pub use job::{ConvertImageRequest, Job, JobId, JobState};
pub use machine::{
    Arch, Backend, ConsoleKind, CreateMachineRequest, DiskBus, DiskMedia, DiskSpec, Firmware,
    GuestOs, ImageRef, Machine, MachineId, MachineSpec, MachineState, NetworkSpec, NewDisk,
    PowerAction, PowerRequest, Thumbnail, ThumbnailKind,
};
pub use preferences::Preferences;
