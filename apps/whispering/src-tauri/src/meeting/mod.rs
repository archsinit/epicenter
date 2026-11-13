//! Native meeting worker, progressive audio storage and capture backends.
//! Application IPC is registered separately; importing never starts capture.

pub mod audio;
pub mod audio_recovery;
#[cfg(target_os = "macos")]
pub mod backend_macos;
pub mod capture_process;
pub mod control;
pub mod controller;
pub mod library;
pub mod local_folder;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod quit;
pub mod recovery;
pub mod recovery_transaction;
pub mod request;
pub mod schedule;
pub mod sources;
pub mod storage;
pub mod timeline;
pub mod worker;
