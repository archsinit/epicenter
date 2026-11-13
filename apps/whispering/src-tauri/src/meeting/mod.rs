//! Native meeting worker, progressive audio storage and capture backends.
//! Application commands/UI are not yet wired; importing never starts capture.

pub mod audio;
#[cfg(target_os = "macos")]
pub mod backend_macos;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod schedule;
pub mod storage;
pub mod timeline;
pub mod worker;
