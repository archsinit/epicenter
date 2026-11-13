//! Native meeting worker, progressive audio storage and capture backends.
//! Application commands/UI are not yet wired; importing never starts capture.

pub mod audio;
#[cfg(target_os = "macos")]
pub mod backend_macos;
pub mod capture_process;
pub mod control;
pub mod controller;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod request;
pub mod schedule;
pub mod sources;
pub mod storage;
pub mod timeline;
pub mod worker;
