//! Application capture ownership. Pure state machine: no devices, permissions,
//! webviews or timers. A single native gate arbitrates all dictation methods,
//! native CPAL sessions and the meeting worker, including startup/finalization.

use std::io;

#[derive(Debug, PartialEq, Eq)]
enum Owner {
    Dictation(String),
    Meeting,
}

#[derive(Default)]
pub struct CaptureGate {
    owner: Option<Owner>,
    cpal_open: bool,
    ffmpeg_open: bool,
    ffmpeg_unverified: bool,
    closing: bool,
    sequence: u64,
}
impl CaptureGate {
    fn available(&self) -> io::Result<()> {
        if self.closing {
            return Err(io::Error::other("Whispering Intel is shutting down"));
        }
        Ok(())
    }
    pub fn begin_dictation(&mut self) -> io::Result<String> {
        self.available()?;
        if self.owner.is_some() || self.cpal_open || self.has_ffmpeg() {
            return Err(io::Error::other(
                "Stop the active meeting or dictation before recording again",
            ));
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("Capture token limit reached"))?;
        let token = format!("dictation-{}", self.sequence);
        self.owner = Some(Owner::Dictation(token.clone()));
        Ok(token)
    }
    pub fn end_dictation(&mut self, token: &str) -> io::Result<()> {
        if self.cpal_open || self.has_ffmpeg() {
            return Err(io::Error::other(
                "Recording input has not confirmed closure",
            ));
        }
        match &self.owner {
            Some(Owner::Dictation(current)) if current == token => {
                self.owner = None;
                Ok(())
            }
            _ => Err(io::Error::other(
                "Capture lease does not match the active dictation",
            )),
        }
    }
    pub fn begin_meeting(&mut self) -> io::Result<()> {
        self.available()?;
        if self.owner.is_some() || self.cpal_open || self.has_ffmpeg() {
            return Err(io::Error::other(
                "Stop dictation/VAD and close its recording session before starting a meeting",
            ));
        }
        self.owner = Some(Owner::Meeting);
        Ok(())
    }
    pub fn end_meeting(&mut self) {
        if self.owner == Some(Owner::Meeting) {
            self.owner = None;
        }
    }
    pub fn ensure_cpal_allowed(&self) -> io::Result<()> {
        self.available()?;
        if self.owner == Some(Owner::Meeting) || self.has_ffmpeg() {
            return Err(io::Error::other(
                "Stop the meeting before starting dictation",
            ));
        }
        Ok(())
    }
    pub fn ensure_ffmpeg_allowed(&self) -> io::Result<()> {
        self.available()?;
        if self.owner == Some(Owner::Meeting) || self.cpal_open || self.has_ffmpeg() {
            return Err(io::Error::other(
                "Stop the active capture before starting FFmpeg",
            ));
        }
        Ok(())
    }
    pub fn set_ffmpeg_open(&mut self, open: bool) {
        self.ffmpeg_open = open;
    }
    pub fn note_unverified_ffmpeg(&mut self) {
        self.ffmpeg_unverified = true;
    }
    pub fn has_ffmpeg(&self) -> bool {
        self.ffmpeg_open || self.ffmpeg_unverified
    }
    pub fn set_cpal_open(&mut self, open: bool) {
        self.cpal_open = open;
    }
    pub fn begin_shutdown(&mut self) {
        self.closing = true;
    }
    pub fn cancel_shutdown(&mut self) {
        self.closing = false;
    }
    pub fn is_closing(&self) -> bool {
        self.closing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn meeting_excludes_dictation_and_direct_cpal_until_released() {
        let mut gate = CaptureGate::default();
        gate.begin_meeting().unwrap();
        assert!(gate.begin_dictation().is_err());
        assert!(gate.ensure_cpal_allowed().is_err());
        assert!(gate.begin_meeting().is_err());
        gate.end_meeting();
        assert!(gate.begin_dictation().is_ok());
    }
    #[test]
    fn lease_is_held_through_starting_stopping_and_errors_until_explicit_release() {
        let mut gate = CaptureGate::default();
        let token = gate.begin_dictation().unwrap();
        assert!(gate.begin_meeting().is_err());
        assert!(gate.begin_dictation().is_err());
        assert!(gate.end_dictation("wrong-token").is_err());
        assert!(gate.begin_meeting().is_err());
        gate.end_dictation(&token).unwrap();
        gate.begin_meeting().unwrap();
        assert!(gate.end_dictation(&token).is_err());
        assert!(gate.begin_dictation().is_err());
    }
    #[test]
    fn initialized_cpal_stream_blocks_meeting_even_without_a_frontend_lease() {
        let mut gate = CaptureGate::default();
        gate.set_cpal_open(true);
        assert!(gate.begin_meeting().is_err());
        assert!(gate.begin_dictation().is_err());
        gate.set_cpal_open(false);
        gate.begin_meeting().unwrap();
    }
    #[test]
    fn old_release_cannot_release_a_new_lease() {
        let mut gate = CaptureGate::default();
        let old = gate.begin_dictation().unwrap();
        gate.end_dictation(&old).unwrap();
        let new = gate.begin_dictation().unwrap();
        assert_ne!(old, new);
        assert!(gate.end_dictation(&old).is_err());
        assert!(gate.begin_meeting().is_err());
        gate.end_dictation(&new).unwrap();
    }
    #[test]
    fn shutdown_prevents_new_capture_but_allows_cleanup_and_cancel() {
        let mut gate = CaptureGate::default();
        let token = gate.begin_dictation().unwrap();
        gate.begin_shutdown();
        gate.end_dictation(&token).unwrap();
        assert!(gate.begin_dictation().is_err());
        assert!(gate.begin_meeting().is_err());
        assert!(gate.ensure_cpal_allowed().is_err());
        gate.cancel_shutdown();
        assert!(gate.begin_meeting().is_ok());
    }
}
