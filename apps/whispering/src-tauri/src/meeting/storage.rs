//! Local-only meeting assets. Nothing in this module invokes a provider or the
//! dictation delivery pipeline. Unfinished directories and unregistered assets
//! are deliberately preserved for recovery after a crash or disk failure.

use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const MANIFEST_VERSION: u32 = 1;
pub(crate) const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MeetingMode {
    Microphone,
    SystemAndMicrophone,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MeetingStatus {
    Recording,
    Completed,
    Interrupted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScreenshotOrigin {
    Automatic,
    Manual,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioTrack {
    #[default]
    Mixed,
    Microphone,
    System,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioChunk {
    pub path: String,
    #[serde(default)]
    pub track: AudioTrack,
    pub offset_ms: u64,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    pub path: String,
    pub offset_ms: u64,
    pub origin: ScreenshotOrigin,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingManifest {
    pub version: u32,
    pub id: String,
    pub title: String,
    pub mode: MeetingMode,
    pub microphone_id: String,
    pub display_id: Option<u32>,
    pub started_at_unix_ms: u64,
    pub duration_ms: Option<u64>,
    pub status: MeetingStatus,
    pub audio: Vec<AudioChunk>,
    pub screenshots: Vec<Screenshot>,
}

impl MeetingManifest {
    pub fn new(
        id: String,
        title: String,
        mode: MeetingMode,
        microphone_id: String,
        display_id: Option<u32>,
        started_at_unix_ms: u64,
    ) -> Self {
        Self {
            version: MANIFEST_VERSION,
            id,
            title,
            mode,
            microphone_id,
            display_id,
            started_at_unix_ms,
            duration_ms: None,
            status: MeetingStatus::Recording,
            audio: Vec::new(),
            screenshots: Vec::new(),
        }
    }
}

/// A progressively written WAV owned by this meeting. The capture backend must
/// finalize its WAV header before commit. Dropping it does not delete the file.
/// The next open can still recover audio even if the manifest update never ran.
pub struct PendingAudioChunk {
    file: File,
    relative_path: String,
    directory: PathBuf,
    offset_ms: u64,
    track: AudioTrack,
}

impl PendingAudioChunk {
    pub fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    pub(crate) fn writer_file(&self) -> io::Result<File> {
        self.file.try_clone()
    }
}

/// A screenshot file reserved by the storage owner, filled on the screenshot
/// thread, then registered on the owner thread. Large PNG writes never block
/// draining the microphone/system audio queues.
pub(crate) struct PendingScreenshot {
    file: File,
    relative_path: String,
    directory: PathBuf,
}
impl PendingScreenshot {
    pub(crate) fn write_png(&mut self, png: &[u8]) -> io::Result<()> {
        if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(invalid("Screenshot must be PNG data"));
        }
        self.file.write_all(png)?;
        self.file.sync_all()
    }
    pub(crate) fn discard(self) -> io::Result<()> {
        drop(self.file);
        fs::remove_file(self.directory.join(self.relative_path))
    }
}

pub struct MeetingStore {
    directory: PathBuf,
    manifest: MeetingManifest,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 96 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn valid_asset(path: &str, folder: &str, extension: &str) -> bool {
    path.strip_prefix(&format!("{folder}/"))
        .and_then(|name| name.strip_suffix(extension))
        .map(|stem| !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or(false)
}

/// Recheck the file contents, not just the .wav extension. This rejects a
/// renamed screenshot and headers left unfinished by a crashed audio writer.
fn validate_wav(file: &mut File) -> io::Result<()> {
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0; 12];
    file.read_exact(&mut header)?;
    let declared_bytes = u32::from_le_bytes(header[4..8].try_into().unwrap()) as u64 + 8;
    if &header[..4] != b"RIFF"
        || &header[8..] != b"WAVE"
        || declared_bytes != file.metadata()?.len()
        || declared_bytes < 44
    {
        return Err(invalid("Audio must be a finalized WAV file"));
    }
    file.seek(SeekFrom::Start(0))?;
    {
        let reader =
            hound::WavReader::new(&mut *file).map_err(|_| invalid("Invalid WAV audio format"))?;
        if reader.len() == 0 || reader.spec().sample_rate == 0 {
            return Err(invalid("Audio chunk contains no samples"));
        }
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(())
}

pub(crate) fn private_directory(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn private_new_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Write a new file, sync it, then atomically replace the old file. On failure,
/// preserve the previous manifest and leave any temporary file for recovery.
pub(crate) fn atomic_write(directory: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let temporary = loop {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(".{name}-{}-{sequence}.tmp", std::process::id()));
        match private_new_file(&path) {
            Ok(file) => break (path, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let (path, mut file) = temporary;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(path, directory.join(name))?;
    #[cfg(unix)]
    File::open(directory)?.sync_all()?;
    Ok(())
}

impl MeetingStore {
    /// `root` must be the app-owned meetings directory. IDs, not user titles,
    /// determine paths; create_new semantics prevent replacing an old meeting.
    pub fn create(root: &Path, manifest: MeetingManifest) -> io::Result<Self> {
        validate_manifest(&manifest)?;
        if manifest.status != MeetingStatus::Recording
            || !manifest.audio.is_empty()
            || !manifest.screenshots.is_empty()
            || manifest.duration_ms.is_some()
        {
            return Err(invalid("New meetings must be empty and recording"));
        }
        if !root.exists() {
            private_directory(root)?;
        }
        let directory = root.join(&manifest.id);
        private_directory(&directory)?;
        private_directory(&directory.join("audio"))?;
        private_directory(&directory.join("screenshots"))?;
        let store = Self {
            directory,
            manifest,
        };
        store.persist(&store.manifest)?;
        Ok(store)
    }

    pub fn manifest(&self) -> &MeetingManifest {
        &self.manifest
    }

    fn persist(&self, manifest: &MeetingManifest) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(manifest)?;
        atomic_write(&self.directory, "meeting.json", &bytes)
    }

    fn update(&mut self, manifest: MeetingManifest) -> io::Result<()> {
        self.persist(&manifest)?;
        self.manifest = manifest;
        Ok(())
    }

    fn require_recording(&self) -> io::Result<()> {
        if self.manifest.status != MeetingStatus::Recording {
            return Err(invalid("Meeting is no longer recording"));
        }
        Ok(())
    }

    /// create_new also skips orphaned assets left by a previous failed update.
    fn new_asset(&self, folder: &str, extension: &str, start: usize) -> io::Result<(String, File)> {
        let mut sequence = start;
        loop {
            let relative = format!("{folder}/{sequence:08}{extension}");
            match private_new_file(&self.directory.join(&relative)) {
                Ok(file) => return Ok((relative, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    sequence = sequence
                        .checked_add(1)
                        .ok_or_else(|| invalid("Asset limit reached"))?;
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub fn begin_audio_chunk(&self, offset_ms: u64) -> io::Result<PendingAudioChunk> {
        self.begin_track_chunk(offset_ms, AudioTrack::Mixed)
    }

    pub(crate) fn open_registered_audio(&self, index: usize) -> io::Result<File> {
        let audio = self
            .manifest
            .audio
            .get(index)
            .ok_or_else(|| invalid("Audio chunk not found"))?;
        open_local_audio(&self.directory, audio)
    }

    pub(crate) fn begin_track_chunk(
        &self,
        offset_ms: u64,
        track: AudioTrack,
    ) -> io::Result<PendingAudioChunk> {
        self.require_recording()?;
        let (relative_path, file) = self.new_asset("audio", ".wav", self.manifest.audio.len())?;
        // A crash can happen before this chunk reaches the main manifest. Keep
        // its source identity and timeline offset beside the progressive WAV;
        // preserving samples alone would lose mic/system alignment on recovery.
        let mut recovery = private_new_file(&self.directory.join(format!("{relative_path}.json")))?;
        serde_json::to_writer(
            &mut recovery,
            &serde_json::json!({
                "version": MANIFEST_VERSION, "path": relative_path, "track": track, "offsetMs": offset_ms
            }),
        )?;
        recovery.sync_all()?;
        #[cfg(unix)]
        File::open(self.directory.join("audio"))?.sync_all()?;
        Ok(PendingAudioChunk {
            file,
            relative_path,
            directory: self.directory.clone(),
            offset_ms,
            track,
        })
    }

    pub fn commit_audio_chunk(
        &mut self,
        mut chunk: PendingAudioChunk,
        duration_ms: u64,
    ) -> io::Result<()> {
        self.require_recording()?;
        if chunk.directory != self.directory || duration_ms == 0 {
            return Err(invalid(
                "Audio chunk does not belong to this meeting or is empty",
            ));
        }
        chunk.file.flush()?;
        chunk.file.sync_all()?;
        validate_wav(&mut chunk.file)?;
        let mut manifest = self.manifest.clone();
        manifest.audio.push(AudioChunk {
            path: chunk.relative_path,
            track: chunk.track,
            offset_ms: chunk.offset_ms,
            duration_ms,
        });
        self.update(manifest)
    }

    pub fn save_screenshot(
        &mut self,
        png: &[u8],
        offset_ms: u64,
        origin: ScreenshotOrigin,
    ) -> io::Result<()> {
        let mut pending = self.begin_screenshot()?;
        pending.write_png(png)?;
        self.commit_screenshot(pending, offset_ms, origin)
    }

    pub(crate) fn begin_screenshot(&self) -> io::Result<PendingScreenshot> {
        self.require_recording()?;
        let (relative_path, file) =
            self.new_asset("screenshots", ".png", self.manifest.screenshots.len())?;
        Ok(PendingScreenshot {
            file,
            relative_path,
            directory: self.directory.clone(),
        })
    }

    pub(crate) fn commit_screenshot(
        &mut self,
        pending: PendingScreenshot,
        offset_ms: u64,
        origin: ScreenshotOrigin,
    ) -> io::Result<()> {
        self.require_recording()?;
        if pending.directory != self.directory {
            return Err(invalid("Screenshot does not belong to this meeting"));
        }
        let mut manifest = self.manifest.clone();
        manifest.screenshots.push(Screenshot {
            path: pending.relative_path,
            offset_ms,
            origin,
        });
        self.update(manifest)
    }

    /// The capture controller must drain pending audio and screenshots first.
    /// Errors leave the directory recoverable; no source file is ever removed.
    pub fn finish(&mut self, duration_ms: u64, interrupted: bool) -> io::Result<()> {
        self.require_recording()?;
        let last_asset_ms = self
            .manifest
            .audio
            .iter()
            .map(|audio| audio.offset_ms.saturating_add(audio.duration_ms))
            .chain(
                self.manifest
                    .screenshots
                    .iter()
                    .map(|image| image.offset_ms),
            )
            .max()
            .unwrap_or(0);
        if duration_ms < last_asset_ms {
            return Err(invalid("Meeting duration cannot precede its assets"));
        }
        let mut manifest = self.manifest.clone();
        manifest.duration_ms = Some(duration_ms);
        manifest.status = if interrupted {
            MeetingStatus::Interrupted
        } else {
            MeetingStatus::Completed
        };
        self.update(manifest)
    }
}

fn validate_manifest(manifest: &MeetingManifest) -> io::Result<()> {
    if manifest.version != MANIFEST_VERSION || !valid_id(&manifest.id) {
        return Err(invalid("Unsupported meeting version or invalid ID"));
    }
    const MAX_SAFE_MS: u64 = 9_007_199_254_740_991;
    if manifest.started_at_unix_ms > 8_640_000_000_000_000
        || manifest
            .duration_ms
            .is_some_and(|duration| duration > MAX_SAFE_MS)
        || manifest.audio.iter().any(|asset| {
            !valid_asset(&asset.path, "audio", ".wav")
                || asset.offset_ms > MAX_SAFE_MS
                || asset.duration_ms == 0
                || asset.duration_ms > MAX_SAFE_MS
        })
        || manifest.screenshots.iter().any(|asset| {
            !valid_asset(&asset.path, "screenshots", ".png") || asset.offset_ms > MAX_SAFE_MS
        })
    {
        return Err(invalid("Invalid local meeting asset path"));
    }
    Ok(())
}

pub(crate) fn checked_meeting_directory(root: &Path, id: &str) -> io::Result<PathBuf> {
    if !valid_id(id) {
        return Err(invalid("Invalid meeting ID"));
    }
    for directory in [root.to_path_buf(), root.join(id)] {
        let metadata = fs::symlink_metadata(&directory)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid(
                "Meeting storage must use real directories, not symbolic links",
            ));
        }
    }
    let directory = root.join(id);
    if directory.canonicalize()? != root.canonicalize()?.join(id) {
        return Err(invalid("Meeting directory is outside local storage"));
    }
    Ok(directory)
}

// Callers supply storage-validated relative asset names, never raw IPC paths.
pub(crate) fn open_local_file(directory: &Path, relative: &str) -> io::Result<File> {
    let path = directory.join(relative);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || path.canonicalize()? != directory.canonicalize()?.join(relative)
    {
        return Err(invalid(
            "Meeting asset must be a local regular file, not a symbolic link",
        ));
    }
    File::open(path)
}

fn open_local_audio(directory: &Path, audio: &AudioChunk) -> io::Result<File> {
    let mut file = open_local_file(directory, &audio.path)?;
    validate_wav(&mut file)?;
    Ok(file)
}

/// Read-only saved meeting. Loading does not restart capture, rewrite status,
/// remove orphan files, or initiate transcription. Recording status on disk
/// means an unfinished session when no live native controller owns it.
pub struct SavedMeeting {
    directory: PathBuf,
    manifest: MeetingManifest,
}

impl SavedMeeting {
    pub fn manifest(&self) -> &MeetingManifest {
        &self.manifest
    }

    pub fn load(root: &Path, id: &str) -> io::Result<Self> {
        let directory = checked_meeting_directory(root, id)?;
        let mut file = open_local_file(&directory, "meeting.json")?;
        if file.metadata()?.len() > MAX_MANIFEST_BYTES {
            return Err(invalid(
                "Meeting manifest is too large to browse safely; files were retained",
            ));
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(invalid("Meeting manifest grew beyond its read limit"));
        }
        let manifest: MeetingManifest = serde_json::from_slice(&bytes)?;
        validate_manifest(&manifest)?;
        if manifest.id != id {
            return Err(invalid("Meeting ID does not match its directory"));
        }
        Ok(Self {
            directory,
            manifest,
        })
    }

    /// Local reference only. This API is deliberately separate from audio-only
    /// transcription and accepts an index, never a webview-supplied file path.
    pub fn open_screenshot_for_local_preview(&self, index: usize) -> io::Result<File> {
        let image = self
            .manifest
            .screenshots
            .get(index)
            .ok_or_else(|| invalid("Screenshot not found"))?;
        let mut file = open_local_file(&self.directory, &image.path)?;
        let mut header = [0; 8];
        file.read_exact(&mut header)?;
        if &header != b"\x89PNG\r\n\x1a\n" {
            return Err(invalid("Screenshot is not PNG data"));
        }
        file.seek(SeekFrom::Start(0))?;
        Ok(file)
    }

    /// Only registered audio can be selected for transcription. Callers cannot
    /// supply a screenshot path or enumerate the entire directory for upload.
    pub fn open_audio_for_transcription(&self, index: usize) -> io::Result<File> {
        let mut file = self.open_mixed_audio_candidate(index)?;
        validate_wav(&mut file)?;
        Ok(file)
    }

    /// Preview reads must bound the exact byte copy BEFORE parsing its WAV
    /// structure. This checks registration/track/path/status only; the library
    /// validates the bounded snapshot before returning any bytes.
    pub(crate) fn open_mixed_audio_candidate(&self, index: usize) -> io::Result<File> {
        if self.manifest.status == MeetingStatus::Recording {
            return Err(invalid(
                "Finish or recover the meeting before transcription",
            ));
        }
        let audio = self
            .manifest
            .audio
            .get(index)
            .ok_or_else(|| invalid("Audio chunk not found"))?;
        if audio.track != AudioTrack::Mixed {
            return Err(invalid("Raw source tracks are not transcription inputs"));
        }
        open_local_file(&self.directory, &audio.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "whispering-meeting-{}-{}",
                std::process::id(),
                TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            private_directory(&path).unwrap();
            Self(path)
        }
        fn create(&self) -> MeetingStore {
            MeetingStore::create(&self.0.join("meetings"), manifest()).unwrap()
        }
        fn load(&self) -> SavedMeeting {
            SavedMeeting::load(&self.0.join("meetings"), "test-123").unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn manifest() -> MeetingManifest {
        MeetingManifest {
            version: MANIFEST_VERSION,
            id: "test-123".into(),
            title: "Meeting / title is not a path".into(),
            mode: MeetingMode::SystemAndMicrophone,
            microphone_id: "default".into(),
            display_id: Some(1),
            started_at_unix_ms: 1_000,
            duration_ms: None,
            status: MeetingStatus::Recording,
            audio: vec![],
            screenshots: vec![],
        }
    }
    fn audio(store: &MeetingStore) -> PendingAudioChunk {
        let mut chunk = store.begin_audio_chunk(0).unwrap();
        // A real tiny 16kHz mono PCM WAV with one sample (46 bytes).
        chunk.file_mut().write_all(b"RIFF\x26\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x80\x3e\0\0\0\x7d\0\0\x02\0\x10\0data\x02\0\0\0\0\0").unwrap();
        chunk
    }
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfixture";

    #[test]
    fn create_persists_local_metadata_without_using_title_as_path() {
        let fixture = Fixture::new();
        let store = fixture.create();
        assert_eq!(fixture.load().manifest.title, store.manifest().title);
        assert_eq!(store.manifest().status, MeetingStatus::Recording);
        assert!(store.directory.join("screenshots").is_dir());
    }

    #[test]
    fn rejects_path_traversal_and_duplicate_meeting_ids() {
        let fixture = Fixture::new();
        let store = fixture.create();
        assert!(MeetingStore::create(&fixture.0.join("meetings"), manifest()).is_err());
        assert!(SavedMeeting::load(&fixture.0, "../outside").is_err());
        let mut bad = manifest();
        bad.id = "../outside".into();
        assert!(MeetingStore::create(&fixture.0, bad).is_err());
        assert!(store.directory.join("meeting.json").is_file());
    }

    #[test]
    fn same_timestamp_screenshots_have_unique_files_and_origins() {
        let fixture = Fixture::new();
        let mut store = fixture.create();
        store
            .save_screenshot(PNG, 300_000, ScreenshotOrigin::Manual)
            .unwrap();
        store
            .save_screenshot(PNG, 300_000, ScreenshotOrigin::Automatic)
            .unwrap();
        let saved = fixture.load();
        assert_ne!(
            saved.manifest.screenshots[0].path,
            saved.manifest.screenshots[1].path
        );
        assert_eq!(
            saved.manifest.screenshots[0].origin,
            ScreenshotOrigin::Manual
        );
        assert_eq!(saved.manifest.screenshots[1].offset_ms, 300_000);
    }

    #[test]
    fn unfinished_audio_survives_drop_and_is_not_silently_resumed_or_uploaded() {
        let fixture = Fixture::new();
        let store = fixture.create();
        let pending = audio(&store);
        let path = store.directory.join(&pending.relative_path);
        pending.file.sync_all().unwrap();
        drop(pending);
        drop(store);
        let saved = fixture.load();
        assert_eq!(saved.manifest.status, MeetingStatus::Recording);
        assert!(path.is_file());
        assert!(saved.open_audio_for_transcription(0).is_err());
    }

    #[test]
    fn only_registered_audio_can_be_opened_for_transcription() {
        let fixture = Fixture::new();
        let mut store = fixture.create();
        let chunk = audio(&store);
        store.commit_audio_chunk(chunk, 1).unwrap();
        store
            .save_screenshot(PNG, 2, ScreenshotOrigin::Manual)
            .unwrap();
        assert!(fixture.load().open_audio_for_transcription(0).is_err());
        store.finish(3, false).unwrap();
        let saved = fixture.load();
        let mut bytes = Vec::new();
        saved
            .open_audio_for_transcription(0)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert!(bytes.starts_with(b"RIFF"));
        assert!(!bytes.starts_with(b"\x89PNG"));
        assert!(saved.open_audio_for_transcription(1).is_err());
    }

    #[test]
    fn orphan_asset_is_preserved_and_not_overwritten() {
        let fixture = Fixture::new();
        let mut store = fixture.create();
        let orphan = store.directory.join("screenshots/00000000.png");
        fs::write(&orphan, b"recover me").unwrap();
        store
            .save_screenshot(PNG, 1, ScreenshotOrigin::Manual)
            .unwrap();
        assert_eq!(fs::read(orphan).unwrap(), b"recover me");
        assert_eq!(
            store.manifest().screenshots[0].path,
            "screenshots/00000001.png"
        );
    }

    #[test]
    fn failed_manifest_update_keeps_previous_state_and_preserves_asset() {
        let fixture = Fixture::new();
        let mut store = fixture.create();
        // Deterministic rename failure, independent of permissions/root runner.
        fs::remove_file(store.directory.join("meeting.json")).unwrap();
        fs::create_dir(store.directory.join("meeting.json")).unwrap();
        assert!(store
            .save_screenshot(PNG, 1, ScreenshotOrigin::Manual)
            .is_err());
        assert!(store.manifest().screenshots.is_empty());
        assert!(store.directory.join("screenshots/00000000.png").is_file());
    }

    #[test]
    fn finalized_meeting_rejects_further_capture_and_incorrect_duration() {
        let fixture = Fixture::new();
        let mut store = fixture.create();
        store
            .save_screenshot(PNG, 300_000, ScreenshotOrigin::Automatic)
            .unwrap();
        assert!(store.finish(1, false).is_err());
        store.finish(301_000, false).unwrap();
        assert_eq!(fixture.load().manifest.status, MeetingStatus::Completed);
        assert!(store.begin_audio_chunk(301_000).is_err());
        assert!(store
            .save_screenshot(PNG, 301_000, ScreenshotOrigin::Manual)
            .is_err());
        assert!(store.finish(302_000, false).is_err());
    }

    #[test]
    fn audio_chunk_cannot_be_committed_to_another_meeting() {
        let first = Fixture::new();
        let second = Fixture::new();
        let one = first.create();
        let mut two = second.create();
        assert!(two.commit_audio_chunk(audio(&one), 1).is_err());
        assert!(two.manifest().audio.is_empty());
    }

    #[test]
    fn malicious_manifest_cannot_turn_screenshot_into_transcription_input() {
        let fixture = Fixture::new();
        let store = fixture.create();
        let mut bad = manifest();
        bad.audio.push(AudioChunk {
            path: "screenshots/00000000.png".into(),
            track: AudioTrack::Mixed,
            offset_ms: 0,
            duration_ms: 1,
        });
        store.persist(&bad).unwrap();
        assert!(SavedMeeting::load(&fixture.0.join("meetings"), "test-123").is_err());
        bad.audio[0].path = "audio/../../private.wav".into();
        store.persist(&bad).unwrap();
        assert!(SavedMeeting::load(&fixture.0.join("meetings"), "test-123").is_err());
    }

    #[test]
    fn renamed_screenshot_and_unfinished_wav_are_rejected_as_audio() {
        let fixture = Fixture::new();
        let mut store = fixture.create();
        let mut renamed = store.begin_audio_chunk(0).unwrap();
        renamed.file_mut().write_all(PNG).unwrap();
        assert!(store.commit_audio_chunk(renamed, 1).is_err());
        let mut unfinished = audio(&store);
        unfinished.file_mut().seek(SeekFrom::Start(4)).unwrap();
        unfinished
            .file_mut()
            .write_all(&u32::MAX.to_le_bytes())
            .unwrap();
        assert!(store.commit_audio_chunk(unfinished, 1).is_err());
        assert!(store.manifest().audio.is_empty());
    }

    #[test]
    fn audio_replaced_with_image_after_recording_is_not_a_transcription_input() {
        let fixture = Fixture::new();
        let mut store = fixture.create();
        let chunk = audio(&store);
        store.commit_audio_chunk(chunk, 1).unwrap();
        store.finish(2, false).unwrap();
        let saved = fixture.load();
        fs::write(store.directory.join(&saved.manifest.audio[0].path), PNG).unwrap();
        assert!(saved.open_audio_for_transcription(0).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_audio_cannot_redirect_transcription_outside_meeting() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let mut store = fixture.create();
        let chunk = audio(&store);
        store.commit_audio_chunk(chunk, 1).unwrap();
        store.finish(2, false).unwrap();
        let path = store.directory.join(&store.manifest().audio[0].path);
        let outside = fixture.0.join("outside.wav");
        fs::rename(&path, &outside).unwrap();
        symlink(&outside, &path).unwrap();
        assert!(fixture.load().open_audio_for_transcription(0).is_err());
    }

    #[test]
    fn interrupted_finalize_preserves_assets() {
        let fixture = Fixture::new();
        let mut store = fixture.create();
        let chunk = audio(&store);
        store.commit_audio_chunk(chunk, 1).unwrap();
        store.finish(2, true).unwrap();
        let saved = fixture.load();
        assert_eq!(saved.manifest.status, MeetingStatus::Interrupted);
        assert!(saved.open_audio_for_transcription(0).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn meeting_directory_and_files_are_private_by_default() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        let store = fixture.create();
        assert_eq!(
            fs::metadata(&store.directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(store.directory.join("meeting.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
