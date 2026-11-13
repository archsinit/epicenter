//! Meeting recording building blocks. The worker/controller and UI are not yet
//! connected; importing this module never starts audio or screenshots.

#[cfg(any(target_os = "macos", test))]
pub mod audio;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod schedule;
pub mod storage;
