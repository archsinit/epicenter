//! Archive untrusted saved FFmpeg metadata, never an exit receipt or a PID to
//! signal. Only called after explicit native confirmation. No audio is opened,
//! deleted, uploaded or interpreted. The live quarantine is not cleared.
use super::storage::{atomic_write, private_directory};
use serde::Serialize;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub const MAX_SAVED_SESSION_BYTES: usize = 65_536;

pub fn validate_saved_metadata(raw: &str) -> io::Result<()> {
    if raw.is_empty() || raw.len() > MAX_SAVED_SESSION_BYTES {
        return Err(io::Error::other(
            "Saved recording metadata must contain 1–65536 bytes; keep it for manual recovery",
        ));
    }
    Ok(())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MetadataArchive<'a> {
    schema_version: u32,
    verification: &'static str,
    restart_required: bool,
    saved_session: &'a str,
}
pub fn archive_confirmed_metadata(root: &Path, raw: &str) -> io::Result<PathBuf> {
    validate_saved_metadata(raw)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    // The root and generated ID are native-owned; no metadata value is a path.
    match std::fs::symlink_metadata(root) {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err(io::Error::other(
                "Recovery archive root must be a real directory",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => private_directory(root)?,
        Err(error) => return Err(error),
    }
    let directory = root.join(format!(
        "ffmpeg-{}-{}",
        now.as_nanos(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    private_directory(&directory)?;
    let archive = MetadataArchive {
        schema_version: 1,
        verification: "user-confirmed-external-stop-not-native-verified",
        restart_required: true,
        saved_session: raw,
    };
    atomic_write(
        &directory,
        "saved-session.json",
        &serde_json::to_vec_pretty(&archive)?,
    )?;
    #[cfg(unix)]
    File::open(root)?.sync_all()?;
    Ok(directory.join("saved-session.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "capture-recovery-test-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            )))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn preserves_exact_malformed_metadata_without_interpreting_paths_or_pid() {
        let fixture = Fixture::new();
        let raw = "{broken JSON ../screenshots/private.png PID=1234";
        let first = archive_confirmed_metadata(&fixture.0, raw).unwrap();
        let second = archive_confirmed_metadata(&fixture.0, raw).unwrap();
        assert_ne!(first, second);
        let saved: serde_json::Value = serde_json::from_reader(File::open(first).unwrap()).unwrap();
        assert_eq!(saved["savedSession"], raw);
        assert_eq!(
            saved["verification"],
            "user-confirmed-external-stop-not-native-verified"
        );
        assert_eq!(saved["restartRequired"], true);
        assert!(saved.get("captureId").is_none());
    }
    #[test]
    fn invalid_size_creates_nothing_and_storage_failure_does_not_claim_success() {
        let fixture = Fixture::new();
        assert!(archive_confirmed_metadata(&fixture.0, "").is_err());
        assert!(
            archive_confirmed_metadata(&fixture.0, &"x".repeat(MAX_SAVED_SESSION_BYTES + 1))
                .is_err()
        );
        assert!(!fixture.0.exists());
        assert!(validate_saved_metadata(&"é".repeat(MAX_SAVED_SESSION_BYTES / 2 + 1)).is_err());
        std::fs::write(&fixture.0, b"not a directory").unwrap();
        assert!(archive_confirmed_metadata(&fixture.0, "saved").is_err());
        assert_eq!(std::fs::read(&fixture.0).unwrap(), b"not a directory");
        std::fs::remove_file(&fixture.0).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn symlinked_archive_root_is_rejected() {
        let target = Fixture::new();
        let link = Fixture::new();
        std::fs::create_dir(&target.0).unwrap();
        std::os::unix::fs::symlink(&target.0, &link.0).unwrap();
        assert!(archive_confirmed_metadata(&link.0, "saved").is_err());
        assert_eq!(std::fs::read_dir(&target.0).unwrap().count(), 0);
        std::fs::remove_file(&link.0).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn archive_and_directories_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        let path = archive_confirmed_metadata(&fixture.0, "saved").unwrap();
        for directory in [&fixture.0, path.parent().unwrap()] {
            assert_eq!(
                std::fs::metadata(directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
