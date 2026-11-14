//! Read-only local library. No capture factories, providers, recovery writes or
//! deletion. Screenshot preview and mixed-audio access have separate entry points.
use super::storage::{checked_meeting_directory, MeetingManifest, MeetingStatus, SavedMeeting};
use serde::Serialize;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

pub const MAX_LIBRARY_ENTRIES: usize = 500;
pub const MAX_AUDIO_PREVIEW_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_SCREENSHOT_PREVIEW_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LibraryState {
    Active,
    Completed,
    Interrupted,
    Unfinished,
    Unavailable,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    pub id: String,
    pub title: String,
    pub state: LibraryState,
    pub started_at_unix_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub audio_chunks: usize,
    pub screenshots: usize,
    pub error: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingLibrary {
    pub entries: Vec<LibraryEntry>,
    pub truncated: bool,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingDetail {
    pub manifest: MeetingManifest,
    pub state: LibraryState,
    pub unregistered_assets: usize,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn state(manifest: &MeetingManifest, active_id: Option<&str>) -> LibraryState {
    if active_id == Some(manifest.id.as_str()) {
        return LibraryState::Active;
    }
    match manifest.status {
        MeetingStatus::Completed => LibraryState::Completed,
        MeetingStatus::Interrupted => LibraryState::Interrupted,
        MeetingStatus::Recording => LibraryState::Unfinished,
    }
}
pub fn list(root: &Path, active_id: Option<&str>) -> io::Result<MeetingLibrary> {
    list_page(root, active_id, None)
}
pub fn list_page(
    root: &Path,
    active_id: Option<&str>,
    after: Option<&str>,
) -> io::Result<MeetingLibrary> {
    if after.is_some_and(|s| s.len() > 1024 || s.contains('\0')) {
        return Err(invalid("Invalid library cursor"));
    }
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(MeetingLibrary {
                entries: vec![],
                truncated: false,
                next_cursor: None,
            })
        }
        Err(error) => return Err(error),
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err(invalid("Meeting library must be a real local directory"))
        }
        Ok(_) => {}
    }
    let mut entries = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if !kind.is_dir() && !kind.is_symlink() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        if after.is_some_and(|cursor| id.as_str() <= cursor) {
            continue;
        }
        names.insert(id);
        if names.len() > MAX_LIBRARY_ENTRIES + 1 {
            names.pop_last();
        }
    }
    let truncated = names.len() > MAX_LIBRARY_ENTRIES;
    if truncated {
        names.pop_last();
    }
    let next_cursor = if truncated {
        names.last().cloned()
    } else {
        None
    };
    for id in names {
        let summary = match SavedMeeting::load(root, &id) {
            Ok(saved) => {
                let manifest = saved.manifest();
                LibraryEntry {
                    id: id.clone(),
                    title: manifest.title.chars().take(240).collect(),
                    state: state(manifest, active_id),
                    started_at_unix_ms: Some(manifest.started_at_unix_ms),
                    duration_ms: manifest.duration_ms,
                    audio_chunks: manifest.audio.len(),
                    screenshots: manifest.screenshots.len(),
                    error: None,
                }
            }
            Err(error) => LibraryEntry {
                id: id.clone(),
                title: id,
                state: LibraryState::Unavailable,
                started_at_unix_ms: None,
                duration_ms: None,
                audio_chunks: 0,
                screenshots: 0,
                error: Some(format!(
                    "Local files retained; metadata cannot be read: {error}"
                )),
            },
        };
        entries.push(summary);
    }
    entries.sort_by(|a, b| {
        b.started_at_unix_ms
            .cmp(&a.started_at_unix_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(MeetingLibrary {
        entries,
        truncated,
        next_cursor,
    })
}
pub fn detail(root: &Path, id: &str, active_id: Option<&str>) -> io::Result<MeetingDetail> {
    let saved = SavedMeeting::load(root, id)?;
    let directory = checked_meeting_directory(root, id)?;
    let manifest = saved.manifest().clone();
    let registered: HashSet<&str> = manifest
        .audio
        .iter()
        .map(|a| a.path.as_str())
        .chain(manifest.screenshots.iter().map(|a| a.path.as_str()))
        .collect();
    let mut unregistered_assets = 0;
    for (folder, extension) in [("audio", ".wav"), ("screenshots", ".png")] {
        let path = directory.join(folder);
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid("Meeting asset folders cannot be symbolic links"));
        }
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(extension)
                && !registered.contains(format!("{folder}/{name}").as_str())
            {
                unregistered_assets += 1;
            }
        }
    }
    Ok(MeetingDetail {
        state: state(&manifest, active_id),
        manifest,
        unregistered_assets,
    })
}
fn bounded_read(mut file: File, maximum: u64) -> io::Result<Vec<u8>> {
    if file.metadata()?.len() > maximum {
        return Err(invalid(
            "Local preview exceeds its size limit; original file was retained",
        ));
    }
    let mut bytes = Vec::new();
    (&mut file).take(maximum + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(invalid("Local asset grew beyond its preview limit"));
    }
    Ok(bytes)
}
/// Finalized, registered mixed WAV only, shared with the existing audio-only
/// storage boundary. Never enumerate a directory or accept a screenshot path.
pub fn read_mixed_audio(
    root: &Path,
    id: &str,
    index: usize,
    active_id: Option<&str>,
) -> io::Result<Vec<u8>> {
    if active_id == Some(id) {
        return Err(invalid(
            "Stop/finalize the active meeting before opening mixed audio",
        ));
    }
    let saved = SavedMeeting::load(root, id)?;
    let bytes = bounded_read(
        saved.open_mixed_audio_candidate(index)?,
        MAX_AUDIO_PREVIEW_BYTES,
    )?;
    // Recheck the exact returned bytes, not a filename or earlier file header.
    if bytes.len() < 44
        || &bytes[..4] != b"RIFF"
        || &bytes[8..12] != b"WAVE"
        || u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8 != bytes.len()
    {
        return Err(invalid("Mixed audio preview is not a finalized WAV"));
    }
    let reader = hound::WavReader::new(std::io::Cursor::new(&bytes))
        .map_err(|_| invalid("Mixed audio preview is invalid"))?;
    if reader.len() == 0 || reader.spec().sample_rate == 0 {
        return Err(invalid("Mixed audio preview is empty"));
    }
    Ok(bytes)
}
/// Local PNG reference only. This method is never called by transcription.
pub fn read_screenshot(root: &Path, id: &str, index: usize) -> io::Result<Vec<u8>> {
    let saved = SavedMeeting::load(root, id)?;
    let bytes = bounded_read(
        saved.open_screenshot_for_local_preview(index)?,
        MAX_SCREENSHOT_PREVIEW_BYTES,
    )?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(invalid("Local screenshot is not PNG data"));
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "library_tests.rs"]
mod tests;
