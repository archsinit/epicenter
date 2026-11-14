//! Local audio-only transcript checkpoints. No provider/capture/image access.
use super::storage::{
    atomic_write, checked_meeting_directory, open_local_file, private_directory, AudioTrack,
    MeetingStatus, SavedMeeting,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
const MAX_BYTES: u64 = 16 * 1024 * 1024;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscriptAudio {
    pub index: usize,
    pub offset_ms: u64,
    pub duration_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscriptSegment {
    pub index: usize,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Transcript {
    pub version: u32,
    pub id: String,
    pub meeting_id: String,
    pub configuration_id: String,
    pub provider_label: String,
    pub audio: Vec<TranscriptAudio>,
    pub segments: Vec<TranscriptSegment>,
}
fn invalid(s: &str) -> io::Error {
    io::Error::other(s)
}
fn valid(s: &str) -> bool {
    !s.is_empty() && s.len() <= 96 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn audio(root: &Path, id: &str, active: Option<&str>) -> io::Result<Vec<TranscriptAudio>> {
    if active == Some(id) {
        return Err(invalid("Stop/finalize capture before transcription"));
    }
    let saved = SavedMeeting::load(root, id)?;
    if saved.manifest().status == MeetingStatus::Recording {
        return Err(invalid("Recover or finish the meeting first"));
    }
    let mut result = Vec::new();
    for (index, a) in saved.manifest().audio.iter().enumerate() {
        if a.track == AudioTrack::Mixed {
            if a.duration_ms > 60_000 {
                return Err(invalid(
                    "Mixed chunk exceeds the one-minute transcription budget",
                ));
            }
            result.push(TranscriptAudio {
                index,
                offset_ms: a.offset_ms,
                duration_ms: a.duration_ms,
            });
        }
    }
    if result.is_empty() {
        return Err(invalid("No registered mixed audio"));
    }
    Ok(result)
}
fn directory(root: &Path, id: &str, create: bool) -> io::Result<PathBuf> {
    let meeting = checked_meeting_directory(root, id)?;
    let p = meeting.join("transcripts");
    if create && !p.exists() {
        private_directory(&p)?;
        #[cfg(unix)]
        fs::File::open(&meeting)?.sync_all()?;
    }
    let m = fs::symlink_metadata(&p)?;
    if !m.is_dir() || m.file_type().is_symlink() {
        return Err(invalid("Transcript folder must be a real local directory"));
    }
    Ok(p)
}
fn load(root: &Path, id: &str, token: &str, active: Option<&str>) -> io::Result<Transcript> {
    if !valid(token) {
        return Err(invalid("Invalid native transcript ID"));
    }
    let directory = directory(root, id, false)?;
    let mut file = open_local_file(&directory, &format!("{token}.json"))?;
    if file.metadata()?.len() > MAX_BYTES {
        return Err(invalid("Transcript exceeds local read budget"));
    }
    let mut bytes = Vec::new();
    (&mut file).take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid("Transcript grew past read budget"));
    }
    let t: Transcript = serde_json::from_slice(&bytes)?;
    if t.version != 1
        || t.id != token
        || t.meeting_id != id
        || t.audio != audio(root, id, active)?
        || t.segments.len() > t.audio.len()
    {
        return Err(invalid("Transcript does not match saved mixed audio"));
    }
    let mut seen = std::collections::BTreeSet::new();
    if t.segments.iter().any(|s| {
        s.text.len() > 1024 * 1024
            || !t.audio.iter().any(|a| a.index == s.index)
            || !seen.insert(s.index)
    }) {
        return Err(invalid("Invalid local transcript segments"));
    }
    Ok(t)
}
fn persist(directory: &Path, t: &Transcript) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(t)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid("Transcript exceeds local write budget"));
    }
    atomic_write(directory, &format!("{}.json", t.id), &bytes)
}
pub fn latest(root: &Path, id: &str, active: Option<&str>) -> io::Result<Option<Transcript>> {
    // No storage is created merely by viewing a transcript.
    let parent = checked_meeting_directory(root, id)?;
    if !parent.join("transcripts").exists() {
        return Ok(None);
    }
    let dir = directory(root, id, false)?;
    let mut names = Vec::new();
    for (e, entry) in fs::read_dir(dir)?.enumerate() {
        if e >= 10_000 {
            return Err(invalid("Transcript history exceeds browsing budget"));
        }
        let n = entry?.file_name().to_string_lossy().into_owned();
        if n.starts_with("transcript-") && n.ends_with(".json") {
            names.push(n);
        }
    }
    names.sort();
    match names.last() {
        None => Ok(None),
        Some(n) => load(root, id, &n[..n.len() - 5], active).map(Some),
    }
}
pub fn begin(
    root: &Path,
    id: &str,
    configuration_id: &str,
    provider_label: &str,
    resume: bool,
    active: Option<&str>,
) -> io::Result<Transcript> {
    if configuration_id.len() != 64
        || !configuration_id.bytes().all(|b| b.is_ascii_hexdigit())
        || provider_label.is_empty()
        || provider_label.len() > 200
    {
        return Err(invalid("Invalid transcription configuration identity"));
    }
    let tracks = audio(root, id, active)?;
    if resume {
        let old = latest(root, id, active)?.ok_or_else(|| invalid("No transcript to resume"))?;
        if old.configuration_id != configuration_id {
            return Err(invalid(
                "Transcription settings changed; explicitly start a new transcript",
            ));
        }
        return Ok(old);
    }
    let directory = directory(root, id, true)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    let token = format!(
        "transcript-{}-{}",
        now.as_nanos(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let t = Transcript {
        version: 1,
        id: token,
        meeting_id: id.to_owned(),
        configuration_id: configuration_id.to_owned(),
        provider_label: provider_label.to_owned(),
        audio: tracks,
        segments: vec![],
    };
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // Reserve the native ID without ever overwriting an existing record.
    options
        .open(directory.join(format!("{}.json", t.id)))?
        .sync_all()?;
    persist(&directory, &t)?;
    Ok(t)
}
pub fn save(
    root: &Path,
    id: &str,
    token: &str,
    index: usize,
    text: &str,
    active: Option<&str>,
) -> io::Result<Transcript> {
    if text.len() > 1024 * 1024 {
        return Err(invalid("Provider text exceeds one-chunk local budget"));
    }
    let mut t = load(root, id, token, active)?;
    if !t.audio.iter().any(|a| a.index == index) {
        return Err(invalid(
            "Only registered mixed chunk indices can receive transcripts",
        ));
    }
    if !t.segments.iter().any(|s| s.index == index) {
        t.segments.push(TranscriptSegment {
            index,
            text: text.to_owned(),
        });
        t.segments.sort_by_key(|s| s.index);
        persist(&directory(root, id, false)?, &t)?;
    }
    Ok(t) // First durable result wins; retry after lost acknowledgement is idempotent.
}

#[cfg(test)]
mod tests {
    use super::super::storage::{MeetingManifest, MeetingMode, MeetingStore};
    use super::*;
    #[test]
    fn checkpoint_is_audio_only_private_idempotent_and_resumable() {
        let root = std::env::temp_dir().join(format!(
            "transcript-test-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let mut store = MeetingStore::create(
            &root,
            MeetingManifest::new(
                "source".into(),
                "test".into(),
                MeetingMode::Microphone,
                "mic".into(),
                None,
                0,
            ),
        )
        .unwrap();
        let mut pending = store.begin_audio_chunk(0).unwrap();
        let mut w = hound::WavWriter::new(
            pending.file_mut(),
            hound::WavSpec {
                channels: 1,
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        w.write_sample(1i16).unwrap();
        w.finalize().unwrap();
        store.commit_audio_chunk(pending, 1).unwrap();
        assert!(begin(&root, "source", &"a".repeat(64), "OpenAI", false, None).is_err());
        let mut raw = store.begin_track_chunk(0, AudioTrack::Microphone).unwrap();
        let mut writer = hound::WavWriter::new(
            raw.file_mut(),
            hound::WavSpec {
                channels: 1,
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        writer.write_sample(2i16).unwrap();
        writer.finalize().unwrap();
        store.commit_audio_chunk(raw, 1).unwrap();
        store.finish(1, false).unwrap();
        assert!(latest(&root, "source", None).unwrap().is_none());
        assert!(!root.join("source/transcripts").exists());
        let t = begin(&root, "source", &"a".repeat(64), "OpenAI", false, None).unwrap();
        assert!(save(&root, "source", &t.id, 99, "wrong", None).is_err());
        assert!(save(
            &root,
            "source",
            &t.id,
            1,
            "raw source is not eligible",
            None
        )
        .is_err());
        assert!(save(&root, "source", &t.id, 0, "text", Some("source")).is_err());
        let t = save(&root, "source", &t.id, 0, "first", None).unwrap();
        let second = save(&root, "source", &t.id, 0, "second", None).unwrap();
        assert_eq!(second.segments[0].text, "first");
        assert_eq!(
            begin(&root, "source", &"a".repeat(64), "OpenAI", true, None)
                .unwrap()
                .id,
            t.id
        );
        assert!(begin(&root, "source", &"b".repeat(64), "OpenAI", true, None).is_err());
        assert!(save(&root, "source", "../outside", 0, "bad", None).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(root.join(format!("source/transcripts/{}.json", t.id)))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let transcripts = root.join("source/transcripts");
            let moved = root.join("moved-transcripts");
            fs::rename(&transcripts, &moved).unwrap();
            symlink(&moved, &transcripts).unwrap();
            assert!(latest(&root, "source", None).is_err());
            assert!(save(&root, "source", &t.id, 0, "escape", None).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }
}
