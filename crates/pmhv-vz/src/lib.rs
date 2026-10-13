//! The Apple Virtualization.framework backend.
//!
//! Virtualization objects aren't thread-safe and this crate creates them for the main queue, so
//! the process using it must run [`run_main_queue`] on its main thread and do everything else
//! (tokio included) on other threads. [`Vz`] is the thread-safe handle to the machine.

mod launch;
#[cfg(target_os = "macos")]
mod macos;

pub use launch::{VzBoot, VzDisk, VzDiskKind, VzLaunch};
#[cfg(target_os = "macos")]
pub use macos::{Vz, VzAction, VzError, VzState, run_main_queue};
