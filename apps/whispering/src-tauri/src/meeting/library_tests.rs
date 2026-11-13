use super::super::storage::{AudioTrack, MeetingMode, MeetingStore, ScreenshotOrigin};
use super::*;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfixture";
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "meeting-library-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )))
    }
    fn store(&self, id: &str) -> MeetingStore {
        if !self.0.exists() {
            fs::create_dir(&self.0).unwrap();
        }
        MeetingStore::create(
            &self.0,
            MeetingManifest::new(
                id.into(),
                "Local meeting".into(),
                MeetingMode::Microphone,
                "fixture".into(),
                Some(1),
                100,
            ),
        )
        .unwrap()
    }
    fn audio(store: &mut MeetingStore, track: AudioTrack) {
        let mut chunk = store.begin_track_chunk(0, track).unwrap();
        {
            let mut writer = hound::WavWriter::new(
                chunk.file_mut(),
                hound::WavSpec {
                    channels: 1,
                    sample_rate: 48000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            writer.write_sample(100_i16).unwrap();
            writer.finalize().unwrap();
        }
        store.commit_audio_chunk(chunk, 1).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn absent_library_does_not_create_storage_or_start_anything() {
    let fixture = Fixture::new();
    assert!(list(&fixture.0, None).unwrap().entries.is_empty());
    assert!(!fixture.0.exists());
}
#[test]
fn active_unfinished_interrupted_and_completed_are_distinct_without_rewriting_metadata() {
    let fixture = Fixture::new();
    let active = fixture.store("active");
    let unfinished = fixture.store("unfinished");
    let mut completed = fixture.store("completed");
    completed.finish(0, false).unwrap();
    let mut interrupted = fixture.store("interrupted");
    interrupted.finish(0, true).unwrap();
    let before = fs::read(fixture.0.join("unfinished/meeting.json")).unwrap();
    let library = list(&fixture.0, Some("active")).unwrap();
    for (id, expected) in [
        ("active", LibraryState::Active),
        ("unfinished", LibraryState::Unfinished),
        ("completed", LibraryState::Completed),
        ("interrupted", LibraryState::Interrupted),
    ] {
        assert_eq!(
            library.entries.iter().find(|e| e.id == id).unwrap().state,
            expected
        );
    }
    assert_eq!(
        fs::read(fixture.0.join("unfinished/meeting.json")).unwrap(),
        before
    );
    drop((active, unfinished));
}
#[test]
fn corrupt_and_missing_manifests_remain_visible_and_orphaned_assets_are_counted_not_deleted() {
    let fixture = Fixture::new();
    let mut store = fixture.store("good");
    store
        .save_screenshot(PNG, 0, ScreenshotOrigin::Manual)
        .unwrap();
    Fixture::audio(&mut store, AudioTrack::Mixed);
    fs::write(fixture.0.join("good/audio/99999999.wav"), b"unfinished").unwrap();
    fs::write(fixture.0.join("good/screenshots/99999999.png"), PNG).unwrap();
    fs::create_dir(fixture.0.join("bad")).unwrap();
    fs::write(fixture.0.join("bad/meeting.json"), b"{broken").unwrap();
    fs::create_dir(fixture.0.join("missing")).unwrap();
    let library = list(&fixture.0, None).unwrap();
    assert_eq!(
        library
            .entries
            .iter()
            .filter(|e| e.state == LibraryState::Unavailable)
            .count(),
        2
    );
    assert_eq!(
        detail(&fixture.0, "good", None)
            .unwrap()
            .unregistered_assets,
        2
    );
    assert_eq!(
        fs::read(fixture.0.join("bad/meeting.json")).unwrap(),
        b"{broken"
    );
    assert_eq!(
        fs::read(fixture.0.join("good/audio/99999999.wav")).unwrap(),
        b"unfinished"
    );
}
#[test]
fn mixed_audio_and_png_are_separate_indexed_boundaries_and_require_stopped_audio() {
    let fixture = Fixture::new();
    let mut store = fixture.store("audio");
    Fixture::audio(&mut store, AudioTrack::Microphone);
    Fixture::audio(&mut store, AudioTrack::Mixed);
    store
        .save_screenshot(PNG, 0, ScreenshotOrigin::Manual)
        .unwrap();
    assert!(read_mixed_audio(&fixture.0, "audio", 1, None).is_err());
    assert_eq!(read_screenshot(&fixture.0, "audio", 0).unwrap(), PNG);
    store.finish(1, false).unwrap();
    assert!(read_mixed_audio(&fixture.0, "audio", 0, None).is_err());
    assert!(read_mixed_audio(&fixture.0, "audio", 1, Some("audio")).is_err());
    assert!(read_mixed_audio(&fixture.0, "audio", 1, None)
        .unwrap()
        .starts_with(b"RIFF"));
    assert!(read_screenshot(&fixture.0, "audio", 1).is_err());
    assert!(detail(&fixture.0, "../audio", None).is_err());
    let audio_path = fixture
        .0
        .join("audio")
        .join(&store.manifest().audio[1].path);
    fs::write(audio_path, PNG).unwrap();
    assert!(read_mixed_audio(&fixture.0, "audio", 1, None).is_err());
    fs::write(
        fixture.0.join("audio/screenshots/00000000.png"),
        b"RIFFnot a PNG",
    )
    .unwrap();
    assert!(read_screenshot(&fixture.0, "audio", 0).is_err());
}
#[test]
fn manifest_and_preview_read_limits_preserve_original_files() {
    let fixture = Fixture::new();
    let mut store = fixture.store("large");
    Fixture::audio(&mut store, AudioTrack::Mixed);
    store
        .save_screenshot(PNG, 0, ScreenshotOrigin::Automatic)
        .unwrap();
    store.finish(1, false).unwrap();
    let audio = fixture.0.join("large/audio/00000000.wav");
    fs::OpenOptions::new()
        .write(true)
        .open(audio)
        .unwrap()
        .set_len(MAX_AUDIO_PREVIEW_BYTES + 1)
        .unwrap();
    assert!(read_mixed_audio(&fixture.0, "large", 0, None)
        .unwrap_err()
        .to_string()
        .contains("size limit"));
    let path = fixture.0.join("large/screenshots/00000000.png");
    let mut file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(MAX_SCREENSHOT_PREVIEW_BYTES + 1).unwrap();
    assert!(read_screenshot(&fixture.0, "large", 0).is_err());
    assert_eq!(
        file.metadata().unwrap().len(),
        MAX_SCREENSHOT_PREVIEW_BYTES + 1
    );
    let manifest = fixture.0.join("large/meeting.json");
    fs::write(
        &manifest,
        vec![b' '; super::super::storage::MAX_MANIFEST_BYTES as usize + 1],
    )
    .unwrap();
    assert!(detail(&fixture.0, "large", None).is_err());
    assert_eq!(
        list(&fixture.0, None).unwrap().entries[0].state,
        LibraryState::Unavailable
    );
    file.write_all(b"retained").unwrap();
}
#[cfg(unix)]
#[test]
fn directory_manifest_asset_and_parent_symlink_escapes_are_rejected() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let outside = Fixture::new();
    let mut store = outside.store("escape");
    Fixture::audio(&mut store, AudioTrack::Mixed);
    store
        .save_screenshot(PNG, 0, ScreenshotOrigin::Manual)
        .unwrap();
    store.finish(1, false).unwrap();
    fs::create_dir(&fixture.0).unwrap();
    symlink(outside.0.join("escape"), fixture.0.join("escape")).unwrap();
    assert_eq!(
        list(&fixture.0, None).unwrap().entries[0].state,
        LibraryState::Unavailable
    );
    assert!(detail(&fixture.0, "escape", None).is_err());
    fs::remove_file(fixture.0.join("escape")).unwrap();
    let mut local = fixture.store("local");
    Fixture::audio(&mut local, AudioTrack::Mixed);
    local
        .save_screenshot(PNG, 0, ScreenshotOrigin::Manual)
        .unwrap();
    local.finish(1, false).unwrap();
    let image = fixture.0.join("local/screenshots/00000000.png");
    fs::remove_file(&image).unwrap();
    symlink(outside.0.join("escape/screenshots/00000000.png"), &image).unwrap();
    assert!(read_screenshot(&fixture.0, "local", 0).is_err());
    fs::remove_dir_all(fixture.0.join("local/audio")).unwrap();
    symlink(
        outside.0.join("escape/audio"),
        fixture.0.join("local/audio"),
    )
    .unwrap();
    assert!(read_mixed_audio(&fixture.0, "local", 0, None).is_err());
    assert!(detail(&fixture.0, "local", None).is_err());
    let local_manifest = fixture.0.join("local/meeting.json");
    fs::write(
        outside.0.join("local-manifest.json"),
        fs::read(&local_manifest).unwrap(),
    )
    .unwrap();
    fs::remove_file(&local_manifest).unwrap();
    symlink(outside.0.join("local-manifest.json"), local_manifest).unwrap();
    assert!(SavedMeeting::load(&fixture.0, "local").is_err());
    let alias = outside.0.join("alias");
    symlink(&fixture.0, &alias).unwrap();
    assert!(list(&alias, None).is_err());
}
#[test]
fn list_cap_is_explicit_and_leaves_unlisted_folders_intact() {
    let fixture = Fixture::new();
    fs::create_dir(&fixture.0).unwrap();
    for index in 0..=MAX_LIBRARY_ENTRIES {
        fs::create_dir(fixture.0.join(format!("entry-{index}"))).unwrap();
    }
    let library = list(&fixture.0, None).unwrap();
    assert_eq!(library.entries.len(), MAX_LIBRARY_ENTRIES);
    assert!(library.truncated);
    assert_eq!(
        fs::read_dir(&fixture.0).unwrap().count(),
        MAX_LIBRARY_ENTRIES + 1
    );
}
