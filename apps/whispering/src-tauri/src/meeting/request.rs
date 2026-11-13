//! Narrow user-action input: storage roots and meeting IDs are native-owned.
use super::storage::{MeetingManifest, MeetingMode};
use serde::Deserialize;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartMeetingParams {
    pub title: String,
    pub mode: MeetingMode,
    pub microphone_id: String,
    pub display_id: Option<u32>,
    pub confirmed: bool,
}
impl StartMeetingParams {
    pub fn into_manifest(self) -> io::Result<MeetingManifest> {
        if !self.confirmed {
            return Err(io::Error::other(
                "Confirm recording consent before starting the meeting",
            ));
        }
        let title = self.title.trim();
        if title.is_empty() || title.chars().count() > 200 {
            return Err(io::Error::other(
                "Meeting title must contain 1 to 200 characters",
            ));
        }
        if self.microphone_id.len() > 512 || self.microphone_id.contains('\0') {
            return Err(io::Error::other("Invalid microphone identifier"));
        }
        if self.display_id == Some(0)
            || (self.mode == MeetingMode::SystemAndMicrophone && self.display_id.is_none())
        {
            return Err(io::Error::other(
                "Choose a display for system audio/screenshots",
            ));
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?;
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let id = format!("meeting-{}-{sequence}", now.as_nanos());
        let started_at = u64::try_from(now.as_millis()).map_err(io::Error::other)?;
        Ok(MeetingManifest::new(
            id,
            title.to_owned(),
            self.mode,
            self.microphone_id,
            self.display_id,
            started_at,
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn params() -> StartMeetingParams {
        StartMeetingParams {
            title: " Meeting ".into(),
            mode: MeetingMode::Microphone,
            microphone_id: String::new(),
            display_id: None,
            confirmed: true,
        }
    }
    #[test]
    fn consent_and_valid_source_are_required_before_native_capture() {
        let mut p = params();
        p.confirmed = false;
        assert!(p.into_manifest().is_err());
        let mut p = params();
        p.mode = MeetingMode::SystemAndMicrophone;
        assert!(p.into_manifest().is_err());
        let mut p = params();
        p.display_id = Some(0);
        assert!(p.into_manifest().is_err());
        let mut p = params();
        p.microphone_id = "bad\0identifier".into();
        assert!(p.into_manifest().is_err());
    }
    #[test]
    fn storage_paths_and_ids_cannot_be_supplied_by_the_webview() {
        let json = r#"{"title":"x","mode":"microphone","microphoneId":"","confirmed":true,"outputFolder":"/other"}"#;
        assert!(serde_json::from_str::<StartMeetingParams>(json).is_err());
        let json = r#"{"title":"x","mode":"microphone","microphoneId":""}"#;
        assert!(serde_json::from_str::<StartMeetingParams>(json).is_err());
    }
    #[test]
    fn unique_native_ids_and_trimmed_titles_do_not_become_paths() {
        let first = params().into_manifest().unwrap();
        let second = params().into_manifest().unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(first.title, "Meeting");
        let mut p = params();
        p.title = "../notes".into();
        assert!(!p.into_manifest().unwrap().id.contains('/'));
    }
    #[test]
    fn title_limit_counts_characters_and_rejects_empty_titles() {
        let mut p = params();
        p.title = "あ".repeat(200);
        assert!(p.into_manifest().is_ok());
        let mut p = params();
        p.title = "a".repeat(201);
        assert!(p.into_manifest().is_err());
        let mut p = params();
        p.title = "  ".into();
        assert!(p.into_manifest().is_err());
    }
}
