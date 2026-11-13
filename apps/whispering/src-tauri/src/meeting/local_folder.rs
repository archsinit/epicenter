//! Explicit local folder reveal, separate from browsing and audio/image reads.
//! Injected OS adapter keeps portable tests from opening Finder. No asset bytes,
//! manifest parsing/repair, capture, upload or webview-supplied path/URL.
use super::storage::checked_meeting_directory;
use serde::Serialize;
use std::io;
use std::path::Path;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealedMeetingFolder {
    pub id: String,
}

pub fn reveal(
    root: &Path,
    id: &str,
    opener: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<RevealedMeetingFolder> {
    // Intentionally allow corrupt/missing manifests: the real local directory
    // still needs manual inspection. Do not follow a linked root/session.
    let path = checked_meeting_directory(root, id)?.canonicalize()?;
    opener(&path)?;
    // An acknowledgement of the request, not proof Finder rendered a window.
    Ok(RevealedMeetingFolder { id: id.to_owned() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "meeting-reveal fixture-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn explicit_reveal_uses_canonical_directory_even_when_manifest_is_damaged_or_absent() {
        let fixture = Fixture::new();
        for id in ["damaged", "missing"] {
            fs::create_dir(fixture.0.join(id)).unwrap();
        }
        let manifest = fixture.0.join("damaged/meeting.json");
        fs::write(&manifest, b"{broken").unwrap();
        for id in ["damaged", "missing"] {
            let mut calls = 0;
            let result = reveal(&fixture.0, id, |path| {
                calls += 1;
                assert!(path.is_absolute());
                assert_eq!(path, fixture.0.join(id).canonicalize()?);
                Ok(())
            })
            .unwrap();
            assert_eq!(calls, 1);
            assert_eq!(result.id, id);
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                serde_json::json!({"id": id})
            );
        }
        assert_eq!(fs::read(manifest).unwrap(), b"{broken");
        assert!(!fixture.0.join("missing/meeting.json").exists());
    }
    #[test]
    fn invalid_missing_or_non_directory_targets_never_call_opener_or_create_storage() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("file"), b"not a folder").unwrap();
        for id in [
            "../outside",
            "/tmp",
            "file:///tmp",
            "-R /tmp",
            "missing",
            "file",
            "",
        ] {
            assert!(reveal(&fixture.0, id, |_| panic!("must not open")).is_err());
        }
        assert!(reveal(&fixture.0.join("absent"), "session", |_| panic!(
            "must not open"
        ))
        .is_err());
        assert!(!fixture.0.join("absent").exists());
    }
    #[test]
    fn os_permission_or_opener_failure_is_returned_without_fallback_or_mutation() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("session")).unwrap();
        let error = reveal(&fixture.0, "session", |_| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "synthetic Finder denial",
            ))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read_dir(fixture.0.join("session")).unwrap().count(), 0);
    }
    #[cfg(unix)]
    #[test]
    fn symlinked_roots_or_sessions_are_rejected_before_opener() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("real")).unwrap();
        symlink(fixture.0.join("real"), fixture.0.join("linked")).unwrap();
        assert!(reveal(&fixture.0, "linked", |_| panic!("must not open")).is_err());
        symlink(&fixture.0, fixture.0.join("linked-root")).unwrap();
        assert!(reveal(&fixture.0.join("linked-root"), "real", |_| panic!(
            "must not open"
        ))
        .is_err());
    }
}
