use super::super::storage::MeetingStatus;
use super::*;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    store: MeetingStore,
}
impl Fixture {
    fn new(mode: MeetingMode) -> Self {
        let root = std::env::temp_dir().join(format!(
            "meeting-audio-recovery-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let store = MeetingStore::create(
            &root.join("meetings"),
            MeetingManifest::new(
                "source".into(),
                "Local meeting".into(),
                mode,
                "mic".into(),
                Some(1),
                123,
            ),
        )
        .unwrap();
        Self { root, store }
    }
    fn meetings(&self) -> PathBuf {
        self.root.join("meetings")
    }
    fn directory(&self) -> PathBuf {
        self.meetings().join("source")
    }
    fn wav(&mut self, track: AudioTrack, offset: u64, samples: &[i16], register: bool) -> PathBuf {
        let pending = self.store.begin_track_chunk(offset, track).unwrap();
        let index = self.store.manifest().audio.len();
        let mut writer = hound::WavWriter::new(
            pending.writer_file().unwrap(),
            hound::WavSpec {
                channels: 1,
                sample_rate: 48_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for sample in samples {
            writer.write_sample(*sample).unwrap();
        }
        writer.finalize().unwrap();
        // Locate by sidecar and skip prior orphan files; store's create_new
        // allocation is intentionally independent of manifest length.
        let mut paths: Vec<_> = fs::read_dir(self.directory().join("audio"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "wav"))
            .collect();
        paths.sort();
        let path = paths.last().unwrap().clone();
        if register {
            self.store
                .commit_audio_chunk(pending, (samples.len() as u64 * 1_000).div_ceil(48_000))
                .unwrap();
            assert_eq!(self.store.manifest().audio.len(), index + 1);
        }
        path
    }
    fn plan(&self) -> io::Result<AudioRecoveryPlan> {
        inspect(&self.meetings(), "source")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn patch(path: &Path, offset: u64, bytes: &[u8]) {
    let mut file = OpenOptions::new().write(true).open(path).unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}
fn tree(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(tree(&path));
        } else {
            result.insert(path.clone(), fs::read(path).unwrap());
        }
    }
    result
}
fn mixed_samples(fixture: &Fixture, id: &str) -> Vec<i16> {
    let saved = SavedMeeting::load(&fixture.meetings(), id).unwrap();
    let index = saved
        .manifest()
        .audio
        .iter()
        .position(|a| a.track == AudioTrack::Mixed)
        .unwrap();
    hound::WavReader::new(saved.open_audio_for_transcription(index).unwrap())
        .unwrap()
        .samples::<i16>()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn orphan_stale_header_and_partial_sample_recover_to_separate_interrupted_copy() {
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    let raw = fixture.wav(AudioTrack::Microphone, 0, &[1000, -2000, 3000], false);
    patch(&raw, 4, &0u32.to_le_bytes());
    patch(&raw, 40, &0u32.to_le_bytes());
    OpenOptions::new()
        .append(true)
        .open(&raw)
        .unwrap()
        .write_all(&[0x7f])
        .unwrap();
    // This is a deliberately invalid image: recovery must not read/parse/copy it.
    fs::write(
        fixture.directory().join("screenshots/00000000.png"),
        b"local-only secret screenshot",
    )
    .unwrap();
    let before = tree(&fixture.directory());
    let plan = fixture.plan().unwrap();
    assert_eq!(tree(&fixture.directory()), before); // inspection is read-only
    assert!(!fixture.meetings().join("copy").exists());
    assert_eq!(plan.preview().unregistered_chunks, 1);
    assert_eq!(plan.preview().normalized_headers, 1);
    assert_eq!(plan.preview().discarded_partial_sample_bytes, 1);
    assert_eq!(plan.preview().duration_ms, 1);
    let preview = plan.reconstruct("copy").unwrap();
    assert_eq!(preview.source_chunks, 1);
    assert_eq!(tree(&fixture.directory()), before);
    let saved = SavedMeeting::load(&fixture.meetings(), "copy").unwrap();
    assert_eq!(saved.manifest().status, MeetingStatus::Interrupted);
    assert_eq!(saved.manifest().duration_ms, Some(1));
    assert!(saved.manifest().screenshots.is_empty());
    assert_eq!(
        fs::read_dir(fixture.meetings().join("copy/screenshots"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        &mixed_samples(&fixture, "copy")[..4],
        &[1000, -2000, 3000, 0]
    );
    let receipt: serde_json::Value = serde_json::from_reader(
        File::open(fixture.meetings().join("copy/recovered-from.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["sourceMeetingId"], "source");
    assert_eq!(receipt["derivedMeetingId"], "copy");
    assert_eq!(receipt["captureCompletenessVerified"], false);
    assert_eq!(receipt["screenshotsRemainInSource"], true);
}

#[test]
fn mixes_registered_and_orphan_sources_on_shared_offsets_and_ignores_old_mixes() {
    let mut fixture = Fixture::new(MeetingMode::SystemAndMicrophone);
    fixture.wav(AudioTrack::Microphone, 0, &[1000; 48], true);
    fixture.wav(AudioTrack::System, 0, &[3000; 48], false);
    fixture.wav(AudioTrack::Mixed, 0, &[30000; 48], false);
    let before = tree(&fixture.directory());
    let plan = fixture.plan().unwrap();
    assert_eq!(plan.preview().source_chunks, 2);
    assert_eq!(plan.preview().ignored_mixed_chunks, 1);
    assert_eq!(plan.preview().microphone_chunks, 1);
    assert_eq!(plan.preview().system_chunks, 1);
    assert_eq!(plan.preview().additional_bytes_estimate, 420);
    plan.reconstruct("copy").unwrap();
    assert_eq!(mixed_samples(&fixture, "copy"), vec![2000; 48]);
    assert_eq!(tree(&fixture.directory()), before);
}

#[test]
fn truncated_committed_audio_uses_only_available_frames_and_reports_mismatch() {
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    let path = fixture.wav(AudioTrack::Microphone, 0, &[1234; 96], true);
    OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_len(44 + 48 * 2)
        .unwrap();
    let plan = fixture.plan().unwrap();
    assert_eq!(plan.preview().normalized_headers, 1);
    assert_eq!(plan.preview().duration_mismatches, 1);
    assert_eq!(plan.preview().duration_ms, 1);
    plan.reconstruct("copy").unwrap();
    assert_eq!(mixed_samples(&fixture, "copy"), vec![1234; 48]);
}

#[test]
fn offset_is_preserved_not_collapsed_to_an_earlier_minute() {
    let mut fixture = Fixture::new(MeetingMode::SystemAndMicrophone);
    fixture.wav(AudioTrack::System, AUDIO_CHUNK_MS, &[2000; 48], false);
    let plan = fixture.plan().unwrap();
    assert_eq!(plan.preview().duration_ms, AUDIO_CHUNK_MS + 1);
    plan.reconstruct("copy").unwrap();
    let saved = SavedMeeting::load(&fixture.meetings(), "copy").unwrap();
    let mixed: Vec<_> = saved
        .manifest()
        .audio
        .iter()
        .enumerate()
        .filter(|(_, a)| a.track == AudioTrack::Mixed)
        .collect();
    assert_eq!(mixed.len(), 2);
    assert_eq!(mixed[1].1.offset_ms, AUDIO_CHUNK_MS);
    let mut reader =
        hound::WavReader::new(saved.open_audio_for_transcription(mixed[1].0).unwrap()).unwrap();
    assert_eq!(reader.samples::<i16>().next().unwrap().unwrap(), 1000);
}

#[test]
fn duplicate_grid_or_conflicting_sidecar_is_rejected_without_writes() {
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    let first = fixture.wav(AudioTrack::Microphone, 0, &[1000], true);
    fixture.wav(AudioTrack::Microphone, 0, &[2000], false);
    assert!(fixture
        .plan()
        .unwrap_err()
        .to_string()
        .contains("Duplicate source/minute"));
    fs::remove_file(fixture.directory().join("audio/00000001.wav")).unwrap();
    patch(&first.with_extension("wav.json"), 0, b"{");
    fs::write(
        first.with_extension("wav.json"),
        br#"{"version":1,"path":"audio/00000000.wav","track":"system","offsetMs":0}"#,
    )
    .unwrap();
    let before = tree(&fixture.directory());
    assert!(fixture.plan().unwrap_err().to_string().contains("disagree"));
    assert_eq!(tree(&fixture.directory()), before);
}

#[test]
fn unregistered_files_require_matching_versioned_source_sidecars() {
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    let path = fixture.wav(AudioTrack::Microphone, 0, &[1000], false);
    let sidecar = path.with_extension("wav.json");
    fs::remove_file(&sidecar).unwrap();
    assert!(fixture
        .plan()
        .unwrap_err()
        .to_string()
        .contains("no native"));
    fs::write(
        &sidecar,
        br#"{"version":1,"path":"screenshots/00000000.png","track":"microphone","offsetMs":0}"#,
    )
    .unwrap();
    assert!(fixture
        .plan()
        .unwrap_err()
        .to_string()
        .contains("version/path"));
    fs::write(&sidecar, vec![b' '; MAX_SIDECAR_BYTES as usize + 1]).unwrap();
    assert!(fixture
        .plan()
        .unwrap_err()
        .to_string()
        .contains("read limit"));
}

#[test]
fn image_substitution_and_non_native_audio_formats_are_rejected() {
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    let path = fixture.wav(AudioTrack::Microphone, 0, &[1000; 48], false);
    patch(&path, 24, &16_000u32.to_le_bytes());
    assert!(fixture.plan().is_err());
    fs::write(
        &path,
        [b"\x89PNG\r\n\x1a\n".as_slice(), &[0u8; 48]].concat(),
    )
    .unwrap();
    assert!(fixture
        .plan()
        .unwrap_err()
        .to_string()
        .contains("not native"));
}

#[test]
fn oversized_source_and_off_grid_or_excessive_offsets_are_rejected_before_copy() {
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    let path = fixture.wav(AudioTrack::Microphone, 1, &[1000], false);
    assert!(fixture.plan().is_err());
    fs::write(path.with_extension("wav.json"), format!(r#"{{"version":1,"path":"audio/00000000.wav","track":"microphone","offsetMs":{MAX_RECOVERY_DURATION_MS}}}"#)).unwrap();
    assert!(fixture.plan().is_err());
    fs::write(
        path.with_extension("wav.json"),
        br#"{"version":1,"path":"audio/00000000.wav","track":"microphone","offsetMs":0}"#,
    )
    .unwrap();
    OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_len(MAX_SOURCE_BYTES + 1)
        .unwrap();
    assert!(fixture
        .plan()
        .unwrap_err()
        .to_string()
        .contains("one native minute"));
    assert!(!fixture.meetings().join("copy").exists());
}

#[test]
fn changed_plan_retains_partial_output_and_originals_without_overwrite_or_delete() {
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    let path = fixture.wav(AudioTrack::Microphone, 0, &[1000], false);
    let plan = fixture.plan().unwrap();
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&[1, 2])
        .unwrap();
    let before = tree(&fixture.directory());
    let error = plan.reconstruct("copy").unwrap_err().to_string();
    assert!(error.contains("Source audio changed"));
    assert!(error.contains("copy"));
    assert_eq!(tree(&fixture.directory()), before);
    assert!(fixture.meetings().join("copy/recovered-from.json").exists());
    assert_eq!(
        SavedMeeting::load(&fixture.meetings(), "copy")
            .unwrap()
            .manifest()
            .status,
        MeetingStatus::Recording
    );
    assert!(fixture.plan().unwrap().reconstruct("copy").is_err());
    fixture.plan().unwrap().reconstruct("second-copy").unwrap();
    assert_eq!(tree(&fixture.directory()), before);
}

#[test]
fn missing_registered_raw_and_metadata_that_would_change_mode_are_not_guessed() {
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    let path = fixture.wav(AudioTrack::Microphone, 0, &[1000], true);
    fs::remove_file(path).unwrap();
    assert!(fixture.plan().unwrap_err().to_string().contains("missing"));
    let mut other = Fixture::new(MeetingMode::Microphone);
    other.wav(AudioTrack::System, 0, &[1000], false);
    assert!(other
        .plan()
        .unwrap_err()
        .to_string()
        .contains("contradicts"));
}

#[test]
fn directory_budget_and_unknown_names_are_explicit_not_silent_truncation() {
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    fixture.wav(AudioTrack::Microphone, 0, &[1000], false);
    assert!(audio_names(&fixture.directory().join("audio"), 1).is_err());
    assert_eq!(
        audio_names(&fixture.directory().join("audio"), 2)
            .unwrap()
            .len(),
        1
    );
    fs::write(
        fixture.directory().join("audio/unidentified.wav"),
        b"unknown",
    )
    .unwrap();
    assert!(fixture
        .plan()
        .unwrap_err()
        .to_string()
        .contains("Unknown WAV"));
}

#[test]
fn new_inventory_or_changed_source_metadata_invalidates_plan_before_creating_output() {
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    fixture.wav(AudioTrack::Microphone, 0, &[1000], false);
    let plan = fixture.plan().unwrap();
    fixture.wav(AudioTrack::Microphone, AUDIO_CHUNK_MS, &[2000], false);
    assert!(plan.reconstruct("copy").is_err());
    assert!(!fixture.meetings().join("copy").exists());
    let plan = fixture.plan().unwrap();
    let path = fixture.directory().join("meeting.json");
    let mut metadata: serde_json::Value =
        serde_json::from_reader(File::open(&path).unwrap()).unwrap();
    metadata["title"] = "Changed title".into();
    fs::write(path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    assert!(plan.reconstruct("copy").is_err());
    assert!(!fixture.meetings().join("copy").exists());
}

#[test]
fn later_copy_failure_keeps_previously_imported_raw_audio_and_provenance() {
    let mut fixture = Fixture::new(MeetingMode::SystemAndMicrophone);
    fixture.wav(AudioTrack::Microphone, 0, &[1000], false);
    let second = fixture.wav(AudioTrack::System, 0, &[2000], false);
    let plan = fixture.plan().unwrap();
    OpenOptions::new()
        .append(true)
        .open(second)
        .unwrap()
        .write_all(&[1, 2])
        .unwrap();
    let before = tree(&fixture.directory());
    assert!(plan.reconstruct("copy").is_err());
    assert_eq!(tree(&fixture.directory()), before);
    let partial = SavedMeeting::load(&fixture.meetings(), "copy").unwrap();
    assert_eq!(partial.manifest().status, MeetingStatus::Recording);
    assert_eq!(partial.manifest().audio.len(), 1);
    assert_eq!(partial.manifest().audio[0].track, AudioTrack::Microphone);
    assert_eq!(
        fs::metadata(fixture.meetings().join("copy/audio/00000000.wav"))
            .unwrap()
            .len(),
        46
    );
    // The partial derived copy itself remains inspectable, with original ID
    // provenance beside it; no data was auto-deleted after the second failure.
    assert_eq!(
        inspect(&fixture.meetings(), "copy")
            .unwrap()
            .preview()
            .source_chunks,
        1
    );
}

#[test]
fn original_id_and_traversal_cannot_overwrite_or_redirect_recovery_output() {
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    fixture.wav(AudioTrack::Microphone, 0, &[1000], false);
    let before = tree(&fixture.directory());
    assert!(fixture.plan().unwrap().reconstruct("source").is_err());
    assert!(fixture.plan().unwrap().reconstruct("../elsewhere").is_err());
    assert_eq!(tree(&fixture.directory()), before);
    assert!(!fixture.root.join("elsewhere").exists());
}

#[test]
fn native_empty_file_is_skipped_only_when_other_raw_samples_are_available() {
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    let empty = fixture.wav(AudioTrack::Microphone, 0, &[], false);
    assert!(fixture
        .plan()
        .unwrap_err()
        .to_string()
        .contains("No recoverable"));
    fixture.wav(AudioTrack::Microphone, AUDIO_CHUNK_MS, &[1000], false);
    assert_eq!(fixture.plan().unwrap().preview().empty_chunks, 1);
    assert_eq!(fs::metadata(empty).unwrap().len(), 44);
}

#[cfg(unix)]
#[test]
fn recovery_does_not_open_even_a_broken_registered_screenshot_directory() {
    use std::os::unix::fs::symlink;
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    fixture.wav(AudioTrack::Microphone, 0, &[1000], true);
    fixture
        .store
        .save_screenshot(
            b"\x89PNG\r\n\x1a\nlocal-only",
            0,
            super::super::storage::ScreenshotOrigin::Manual,
        )
        .unwrap();
    let manifest_before = fs::read(fixture.directory().join("meeting.json")).unwrap();
    let moved = fixture.root.join("retained-screenshots");
    fs::rename(fixture.directory().join("screenshots"), &moved).unwrap();
    symlink(
        fixture.root.join("does-not-exist"),
        fixture.directory().join("screenshots"),
    )
    .unwrap();
    fixture.plan().unwrap().reconstruct("copy").unwrap();
    assert_eq!(
        fs::read(fixture.directory().join("meeting.json")).unwrap(),
        manifest_before
    );
    assert_eq!(
        fs::read(moved.join("00000000.png")).unwrap(),
        b"\x89PNG\r\n\x1a\nlocal-only"
    );
    assert!(SavedMeeting::load(&fixture.meetings(), "copy")
        .unwrap()
        .manifest()
        .screenshots
        .is_empty());
}

#[test]
fn removed_sidecar_invalidates_orphan_plan_without_guessing_the_track() {
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    let raw = fixture.wav(AudioTrack::Microphone, 0, &[1000], false);
    let plan = fixture.plan().unwrap();
    fs::remove_file(raw.with_extension("wav.json")).unwrap();
    assert!(plan.reconstruct("copy").is_err());
}

#[cfg(unix)]
#[test]
fn source_sidecar_and_audio_directory_symlinks_are_rejected() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let mut fixture = Fixture::new(MeetingMode::Microphone);
    let path = fixture.wav(AudioTrack::Microphone, 0, &[1000], false);
    let outside = fixture.root.join("outside.wav");
    fs::copy(&path, &outside).unwrap();
    fs::remove_file(&path).unwrap();
    symlink(&outside, &path).unwrap();
    assert!(fixture.plan().is_err());
    fs::remove_file(&path).unwrap();
    fs::copy(&outside, &path).unwrap();
    let metadata = path.with_extension("wav.json");
    let outside_metadata = fixture.root.join("outside.json");
    fs::rename(&metadata, &outside_metadata).unwrap();
    symlink(&outside_metadata, &metadata).unwrap();
    assert!(fixture.plan().is_err());
    fs::remove_file(&metadata).unwrap();
    fs::rename(outside_metadata, metadata).unwrap();
    fixture.plan().unwrap().reconstruct("copy").unwrap();
    for path in [
        fixture.meetings().join("copy/meeting.json"),
        fixture.meetings().join("copy/recovered-from.json"),
        fixture.meetings().join("copy/audio/00000000.wav"),
    ] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert_eq!(
        fs::metadata(fixture.meetings().join("copy"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let audio = fixture.directory().join("audio");
    let outside_audio = fixture.root.join("outside-audio");
    fs::rename(&audio, &outside_audio).unwrap();
    symlink(outside_audio, audio).unwrap();
    assert!(fixture.plan().is_err());
}
