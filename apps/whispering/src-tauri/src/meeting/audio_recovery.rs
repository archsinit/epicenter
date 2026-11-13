//! Explicit copy-on-recovery engine, NOT exposed through IPC yet. Callers must
//! first establish native exclusive maintenance admission (never infer closure
//! from stable file sizes/mtime). Inspection does not write or resume capture.
//! Recovery writes a new session only; screenshots stay with the original and
//! no provider, dictation delivery, capture factory or permission API is used.
use super::storage::{
    atomic_write, checked_meeting_directory, open_local_file, AudioChunk, AudioTrack,
    MeetingManifest, MeetingMode, MeetingStore, SavedMeeting,
};
use super::timeline::{mix_tracks, AUDIO_CHUNK_MS};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const MAX_RECOVERY_DURATION_MS: u64 = 24 * 60 * 60 * 1_000;
pub const MAX_RECOVERY_DIRECTORY_ENTRIES: usize = 10_000;
const MAX_SOURCE_BYTES: u64 = 44 + 48_000 * 60 * 2;
const MAX_SIDECAR_BYTES: u64 = 1_024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryPreview {
    pub source_id: String,
    pub duration_ms: u64,
    pub source_chunks: usize,
    pub unregistered_chunks: usize,
    pub ignored_mixed_chunks: usize,
    pub empty_chunks: usize,
    pub normalized_headers: usize,
    pub discarded_partial_sample_bytes: usize,
    pub duration_mismatches: usize,
    pub additional_bytes_estimate: u64,
    pub microphone_chunks: usize,
    pub system_chunks: usize,
}

#[derive(Debug)]
struct SourceChunk {
    path: String,
    track: AudioTrack,
    offset_ms: u64,
    frames: u64,
    had_sidecar: bool,
    length: u64,
    modified: SystemTime,
    header: [u8; 44],
}

/// Private source selection: later UI code must retain a native plan/token,
/// not deserialize webview-supplied paths or source classifications.
#[derive(Debug)]
pub struct AudioRecoveryPlan {
    root: PathBuf,
    directory: PathBuf,
    manifest: MeetingManifest,
    sources: Vec<SourceChunk>,
    audio_names: Vec<String>,
    preview: RecoveryPreview,
}
impl AudioRecoveryPlan {
    pub fn preview(&self) -> &RecoveryPreview {
        &self.preview
    }

    /// A deliberate action only, after exclusive native maintenance admission.
    /// `new_id` must be generated natively, never taken from webview input.
    /// Partial output remains recoverable on failure; originals are never edited.
    pub fn reconstruct(self, new_id: &str) -> io::Result<RecoveryPreview> {
        if checked_meeting_directory(&self.root, &self.manifest.id)? != self.directory {
            return Err(invalid("Source storage changed after recovery inspection"));
        }
        let current = SavedMeeting::load(&self.root, &self.manifest.id)?;
        if serde_json::to_vec(current.manifest())? != serde_json::to_vec(&self.manifest)?
            || audio_names(
                &self.directory.join("audio"),
                MAX_RECOVERY_DIRECTORY_ENTRIES,
            )? != self.audio_names
        {
            return Err(invalid(
                "Source metadata/inventory changed after inspection; inspect again",
            ));
        }
        let derived = MeetingManifest::new(
            new_id.to_owned(),
            format!(
                "Recovered: {}",
                self.manifest.title.chars().take(180).collect::<String>()
            ),
            self.manifest.mode.clone(),
            self.manifest.microphone_id.clone(),
            None,
            self.manifest.started_at_unix_ms,
        );
        let mut store = MeetingStore::create(&self.root, derived)?;
        let result = (|| -> io::Result<()> {
            // Persist provenance before importing: even partial copies are
            // identifiable. This is not an assertion of verified/full capture.
            atomic_write(
                &self.root.join(new_id),
                "recovered-from.json",
                &serde_json::to_vec_pretty(&serde_json::json!({
                    "version": 1, "kind": "unfinished-audio-recovery-copy",
                    "sourceMeetingId": self.manifest.id, "derivedMeetingId": new_id,
                    "screenshotsRemainInSource": true,
                    "captureCompletenessVerified": false,
                    "preview": self.preview,
                }))?,
            )?;
            for source in &self.sources {
                let metadata = sidecar(&self.directory, &source.path)?;
                if metadata.is_some() != source.had_sidecar
                    || metadata.is_some_and(|sidecar| {
                        sidecar.track != source.track || sidecar.offset_ms != source.offset_ms
                    })
                {
                    return Err(invalid(
                        "Source sidecar changed after inspection; inspect again",
                    ));
                }
                let mut file = open_local_file(&self.directory, &source.path)?;
                let before = file.metadata()?;
                if before.len() != source.length || before.modified()? != source.modified {
                    return Err(invalid(
                        "Source audio changed after inspection; inspect again",
                    ));
                }
                // One bounded minute at a time. Header/mtime checks catch stale
                // plans, NOT live hardware ownership or authenticated integrity.
                let mut bytes = Vec::new();
                (&mut file)
                    .take(MAX_SOURCE_BYTES + 1)
                    .read_to_end(&mut bytes)?;
                let after = file.metadata()?;
                if bytes.len() as u64 != source.length
                    || after.len() != source.length
                    || after.modified()? != source.modified
                    || bytes[..44] != source.header
                {
                    return Err(invalid(
                        "Source audio changed while copying; originals retained",
                    ));
                }
                let data_bytes = source.frames * 2;
                let mut header = source.header;
                header[4..8].copy_from_slice(&(36 + data_bytes as u32).to_le_bytes());
                header[40..44].copy_from_slice(&(data_bytes as u32).to_le_bytes());
                let mut pending = store.begin_track_chunk(source.offset_ms, source.track)?;
                pending.file_mut().write_all(&header)?;
                pending
                    .file_mut()
                    .write_all(&bytes[44..44 + data_bytes as usize])?;
                store.commit_audio_chunk(pending, (source.frames * 1_000).div_ceil(48_000))?;
            }
            mix_tracks(&mut store, self.preview.duration_ms)?;
            // Recovery never relabels incomplete capture as a successful meeting.
            store.finish(self.preview.duration_ms, true)
        })();
        result.map_err(|error| io::Error::other(format!(
            "Audio recovery failed in local copy {new_id}; original and partial files retained: {error}"
        )))?;
        Ok(self.preview)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Sidecar {
    version: u32,
    path: String,
    track: AudioTrack,
    offset_ms: u64,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn sidecar(directory: &Path, path: &str) -> io::Result<Option<Sidecar>> {
    let relative = format!("{path}.json");
    match fs::symlink_metadata(directory.join(&relative)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
        Ok(_) => {}
    }
    let mut file = open_local_file(directory, &relative)?;
    if file.metadata()?.len() > MAX_SIDECAR_BYTES {
        return Err(invalid("Audio recovery sidecar exceeds its read limit"));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_SIDECAR_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SIDECAR_BYTES {
        return Err(invalid("Audio recovery sidecar grew beyond its read limit"));
    }
    let sidecar: Sidecar = serde_json::from_slice(&bytes)?;
    if sidecar.version != 1 || sidecar.path != path {
        return Err(invalid(
            "Recovery sidecar version/path does not match native audio",
        ));
    }
    Ok(Some(sidecar))
}

/// Accept ONLY the canonical header written by SourceTimeline: no arbitrary
/// RIFF chunks, extensions or non-PCM formats. Length fields may be stale/torn;
/// recover complete persisted sample pairs, not absent/guessed data.
fn native_header(header: &[u8; 44]) -> bool {
    &header[..4] == b"RIFF"
        && &header[8..16] == b"WAVEfmt "
        && header[16..20] == 16u32.to_le_bytes()
        && header[20..22] == 1u16.to_le_bytes()
        && header[22..24] == 1u16.to_le_bytes()
        && header[24..28] == 48_000u32.to_le_bytes()
        && header[28..32] == 96_000u32.to_le_bytes()
        && header[32..34] == 2u16.to_le_bytes()
        && header[34..36] == 16u16.to_le_bytes()
        && &header[36..40] == b"data"
}

fn audio_names(directory: &Path, maximum: usize) -> io::Result<Vec<String>> {
    let metadata = fs::symlink_metadata(directory)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid(
            "Audio recovery requires a real local audio directory",
        ));
    }
    let mut names = Vec::new();
    for (count, entry) in fs::read_dir(directory)?.enumerate() {
        if count == maximum {
            return Err(invalid("Audio directory exceeds the recovery entry limit"));
        }
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|_| invalid("Invalid audio filename"))?;
        if name.ends_with(".wav") {
            let stem = &name[..name.len() - 4];
            if stem.is_empty() || stem.len() > 20 || !stem.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid(
                    "Unknown WAV naming; cannot infer a native source safely",
                ));
            }
            names.push(format!("audio/{name}"));
        }
    }
    names.sort();
    Ok(names)
}

pub fn inspect(root: &Path, id: &str) -> io::Result<AudioRecoveryPlan> {
    let directory = checked_meeting_directory(root, id)?;
    let saved = SavedMeeting::load(root, id)?;
    let manifest = saved.manifest().clone();
    if manifest.title.chars().count() > 200 || manifest.microphone_id.len() > 512 {
        return Err(invalid(
            "Source metadata exceeds native recovery limits; originals retained",
        ));
    }
    let mut registered = BTreeMap::<&str, &AudioChunk>::new();
    for chunk in &manifest.audio {
        if registered.insert(&chunk.path, chunk).is_some() {
            return Err(invalid(
                "Duplicate registered audio path; source identity is ambiguous",
            ));
        }
    }
    let audio_directory = directory.join("audio");
    let names = audio_names(&audio_directory, MAX_RECOVERY_DIRECTORY_ENTRIES)?;
    let inventory = names.clone();
    let mut preview = RecoveryPreview {
        source_id: id.to_owned(),
        duration_ms: 0,
        source_chunks: 0,
        unregistered_chunks: 0,
        ignored_mixed_chunks: 0,
        empty_chunks: 0,
        normalized_headers: 0,
        discarded_partial_sample_bytes: 0,
        duration_mismatches: 0,
        additional_bytes_estimate: 0,
        microphone_chunks: 0,
        system_chunks: 0,
    };
    let mut sources = Vec::new();
    let mut grid = BTreeSet::new();
    let mut found = BTreeSet::new();
    for path in names {
        found.insert(path.clone());
        let registration = registered.get(path.as_str()).copied();
        let metadata = sidecar(&directory, &path)?;
        if let (Some(chunk), Some(sidecar)) = (registration, &metadata) {
            if chunk.track != sidecar.track || chunk.offset_ms != sidecar.offset_ms {
                return Err(invalid(
                    "Manifest and sidecar disagree about audio source/timing",
                ));
            }
        }
        let had_sidecar = metadata.is_some();
        let (track, offset_ms) = match (registration, metadata) {
            (Some(chunk), _) => (chunk.track, chunk.offset_ms),
            (None, Some(sidecar)) => (sidecar.track, sidecar.offset_ms),
            _ => {
                return Err(invalid(
                    "Unregistered WAV has no native source/timing sidecar",
                ))
            }
        };
        if track == AudioTrack::Mixed {
            preview.ignored_mixed_chunks += 1;
            continue; // rebuild from raw sources, never mix mixed audio twice
        }
        if offset_ms % AUDIO_CHUNK_MS != 0 || offset_ms >= MAX_RECOVERY_DURATION_MS {
            return Err(invalid(
                "Recovery source offset exceeds the supported minute grid/day limit",
            ));
        }
        let key = if track == AudioTrack::Microphone {
            0
        } else {
            1
        };
        if !grid.insert((key, offset_ms)) {
            return Err(invalid(
                "Duplicate source/minute; cannot choose which audio to keep",
            ));
        }
        if track == AudioTrack::System && manifest.mode == MeetingMode::Microphone {
            return Err(invalid(
                "System source contradicts microphone-only metadata",
            ));
        }
        let mut file = open_local_file(&directory, &path)?;
        let metadata = file.metadata()?;
        let length = metadata.len();
        if length > MAX_SOURCE_BYTES {
            return Err(invalid(
                "Source WAV exceeds one native minute; original retained",
            ));
        }
        if length < 46 {
            preview.empty_chunks += 1;
            continue; // no complete sample after a native header
        }
        let mut header = [0; 44];
        file.read_exact(&mut header)?;
        if !native_header(&header) {
            return Err(invalid(
                "Recovery source is not native mono 48kHz PCM16 WAV",
            ));
        }
        let frames = (length - 44) / 2;
        let data_bytes = frames * 2;
        preview.normalized_headers += usize::from(
            u32::from_le_bytes(header[4..8].try_into().unwrap()) as u64 + 8 != 44 + data_bytes
                || u32::from_le_bytes(header[40..44].try_into().unwrap()) as u64 != data_bytes,
        );
        preview.discarded_partial_sample_bytes += ((length - 44) % 2) as usize;
        let duration_ms = (frames * 1_000).div_ceil(48_000);
        preview.duration_mismatches +=
            usize::from(registration.is_some_and(|c| c.duration_ms != duration_ms));
        preview.duration_ms = preview.duration_ms.max(offset_ms + duration_ms);
        preview.unregistered_chunks += usize::from(registration.is_none());
        preview.microphone_chunks += usize::from(track == AudioTrack::Microphone);
        preview.system_chunks += usize::from(track == AudioTrack::System);
        preview.additional_bytes_estimate += 44 + data_bytes;
        sources.push(SourceChunk {
            path,
            track,
            offset_ms,
            frames,
            had_sidecar,
            length,
            modified: metadata.modified()?,
            header,
        });
    }
    for chunk in manifest
        .audio
        .iter()
        .filter(|chunk| chunk.track != AudioTrack::Mixed)
    {
        if !found.contains(&chunk.path) {
            return Err(invalid(
                "Registered raw audio is missing; originals retained for manual review",
            ));
        }
    }
    if sources.is_empty() {
        return Err(invalid(
            "No recoverable native raw-source samples; originals retained",
        ));
    }
    preview.source_chunks = sources.len();
    preview.additional_bytes_estimate +=
        preview.duration_ms * 96 + preview.duration_ms.div_ceil(AUDIO_CHUNK_MS) * 44;
    Ok(AudioRecoveryPlan {
        root: root.to_path_buf(),
        directory,
        manifest,
        sources,
        audio_names: inventory,
        preview,
    })
}

#[cfg(test)]
#[path = "audio_recovery_tests.rs"]
mod tests;
