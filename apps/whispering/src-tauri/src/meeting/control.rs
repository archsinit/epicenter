//! Application capture ownership. Pure state machine: no devices, permissions,
//! webviews or timers. A single native gate arbitrates all dictation methods,
//! native CPAL sessions and the meeting worker, including startup/finalization.

use std::io;

#[derive(Debug, PartialEq, Eq)]
enum Owner {
    Dictation(String),
    Meeting,
    Maintenance(u64),
}

/// Native-only token; not serializable or constructible by the webview.
#[derive(Debug)]
pub(crate) struct MaintenanceToken(u64);

#[derive(Default)]
pub struct CaptureGate {
    owner: Option<Owner>,
    cpal_open: bool,
    ffmpeg_open: bool,
    ffmpeg_unverified: bool,
    closing: bool,
    startup_pending: bool,
    sequence: u64,
}
impl CaptureGate {
    pub fn awaiting_restore() -> Self {
        Self {
            startup_pending: true,
            ..Self::default()
        }
    }
    /// Acknowledges inspection of saved frontend FFmpeg state. Never clears
    /// process ownership or an unverified-session fence.
    pub fn finish_startup_restore(&mut self) -> io::Result<()> {
        if self.ffmpeg_unverified {
            return Err(io::Error::other(
                "A previous recording process cannot be verified; capture remains blocked",
            ));
        }
        self.startup_pending = false;
        Ok(())
    }
    fn available(&self) -> io::Result<()> {
        if self.startup_pending {
            return Err(io::Error::other(
                "Recording startup restoration is still pending",
            ));
        }
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
    pub(crate) fn begin_maintenance(&mut self) -> io::Result<MaintenanceToken> {
        self.available()?;
        if self.owner.is_some() || self.cpal_open || self.has_ffmpeg() {
            return Err(io::Error::other(
                "Stop and verify all capture before local audio recovery",
            ));
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("Capture token limit reached"))?;
        self.owner = Some(Owner::Maintenance(self.sequence));
        Ok(MaintenanceToken(self.sequence))
    }
    pub(crate) fn end_maintenance(&mut self, token: &MaintenanceToken) -> io::Result<()> {
        if self.owner != Some(Owner::Maintenance(token.0)) {
            return Err(io::Error::other(
                "Maintenance token does not match active ownership",
            ));
        }
        if self.cpal_open || self.has_ffmpeg() {
            return Err(io::Error::other(
                "Unexpected capture during maintenance; ownership remains quarantined",
            ));
        }
        self.owner = None;
        Ok(())
    }
    pub fn ensure_cpal_allowed(&self) -> io::Result<()> {
        self.available()?;
        if matches!(self.owner, Some(Owner::Meeting | Owner::Maintenance(_))) || self.has_ffmpeg() {
            return Err(io::Error::other(
                "Stop the meeting before starting dictation",
            ));
        }
        Ok(())
    }
    pub fn ensure_ffmpeg_allowed(&self) -> io::Result<()> {
        self.available()?;
        if matches!(self.owner, Some(Owner::Meeting | Owner::Maintenance(_)))
            || self.cpal_open
            || self.has_ffmpeg()
        {
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
    /// Recovery archives metadata only. Even user confirmation is not native
    /// proof of process exit: the current application's quarantine stays set.
    pub fn ensure_metadata_recovery_allowed(&self) -> io::Result<()> {
        if !self.ffmpeg_unverified {
            return Err(io::Error::other(
                "No unverified recording session needs recovery",
            ));
        }
        if self.owner.is_some() || self.cpal_open || self.ffmpeg_open || self.closing {
            return Err(io::Error::other("Stop the known active recorder and cancel shutdown before recovering stale metadata"));
        }
        Ok(())
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
    fn maintenance_is_exclusive_and_only_its_matching_token_can_release_it() {
        let mut gate = CaptureGate::default();
        let first = gate.begin_maintenance().unwrap();
        assert!(gate.begin_meeting().is_err());
        assert!(gate.begin_dictation().is_err());
        assert!(gate.ensure_cpal_allowed().is_err());
        assert!(gate.ensure_ffmpeg_allowed().is_err());
        assert!(gate.begin_maintenance().is_err());
        gate.end_meeting(); // unrelated/stale release cannot clear it
        assert!(gate.end_dictation("dictation-1").is_err());
        gate.end_maintenance(&first).unwrap();
        let second = gate.begin_maintenance().unwrap();
        assert!(gate.end_maintenance(&first).is_err());
        assert!(gate.begin_dictation().is_err());
        gate.begin_shutdown();
        gate.end_maintenance(&second).unwrap();
        assert!(gate.begin_dictation().is_err()); // late completion keeps Quit fence
        gate.cancel_shutdown();
        assert!(gate.begin_dictation().is_ok());
    }
    #[test]
    fn maintenance_refuses_startup_unknown_input_resources_and_shutdown() {
        let mut startup = CaptureGate::awaiting_restore();
        assert!(startup.begin_maintenance().is_err());
        startup.finish_startup_restore().unwrap();
        let token = startup.begin_maintenance().unwrap();
        startup.end_maintenance(&token).unwrap();
        for kind in [
            "dictation",
            "meeting",
            "cpal",
            "ffmpeg",
            "unknown",
            "closing",
        ] {
            let mut gate = CaptureGate::default();
            match kind {
                "dictation" => {
                    gate.begin_dictation().unwrap();
                }
                "meeting" => {
                    gate.begin_meeting().unwrap();
                }
                "cpal" => gate.set_cpal_open(true),
                "ffmpeg" => gate.set_ffmpeg_open(true),
                "unknown" => gate.note_unverified_ffmpeg(),
                _ => gate.begin_shutdown(),
            }
            assert!(gate.begin_maintenance().is_err(), "{kind}");
        }
    }
    #[test]
    fn maintenance_cannot_clear_unexpected_resource_or_unknown_process_flags() {
        let mut gate = CaptureGate::default();
        let token = gate.begin_maintenance().unwrap();
        gate.note_unverified_ffmpeg();
        assert!(gate.end_maintenance(&token).is_err());
        assert!(gate.ensure_metadata_recovery_allowed().is_err());
        assert!(gate.begin_dictation().is_err());
        assert!(gate.finish_startup_restore().is_err());
    }
    #[test]
    fn metadata_recovery_never_releases_quarantine_and_requires_unverified_state() {
        let mut gate = CaptureGate::awaiting_restore();
        assert!(gate.ensure_metadata_recovery_allowed().is_err());
        gate.note_unverified_ffmpeg();
        assert!(gate.ensure_metadata_recovery_allowed().is_ok());
        assert!(gate.finish_startup_restore().is_err());
        assert!(gate.begin_meeting().is_err());
        assert!(gate.ensure_cpal_allowed().is_err());
        assert!(gate.ensure_ffmpeg_allowed().is_err());
        gate.begin_shutdown();
        assert!(gate.ensure_metadata_recovery_allowed().is_err());
        gate.cancel_shutdown();
        assert!(gate.ensure_metadata_recovery_allowed().is_ok());
    }
    #[test]
    fn metadata_recovery_refuses_known_capture_resources_and_owners() {
        let mut gate = CaptureGate::default();
        let token = gate.begin_dictation().unwrap();
        gate.note_unverified_ffmpeg();
        assert!(gate.ensure_metadata_recovery_allowed().is_err());
        // A matching frontend lease is not removed by recovery.
        assert!(gate.end_dictation(&token).is_err());
        let mut meeting = CaptureGate::default();
        meeting.begin_meeting().unwrap();
        meeting.note_unverified_ffmpeg();
        assert!(meeting.ensure_metadata_recovery_allowed().is_err());
        for resource in ["cpal", "ffmpeg"] {
            let mut gate = CaptureGate::awaiting_restore();
            gate.note_unverified_ffmpeg();
            if resource == "cpal" {
                gate.set_cpal_open(true);
            } else {
                gate.set_ffmpeg_open(true);
            }
            assert!(gate.ensure_metadata_recovery_allowed().is_err());
        }
    }
    #[test]
    fn startup_fences_every_capture_entry_until_restore_acknowledged() {
        let mut gate = CaptureGate::awaiting_restore();
        assert!(gate.begin_meeting().is_err());
        assert!(gate.begin_dictation().is_err());
        assert!(gate.ensure_cpal_allowed().is_err());
        assert!(gate.ensure_ffmpeg_allowed().is_err());
        gate.finish_startup_restore().unwrap();
        assert!(gate.ensure_cpal_allowed().is_ok());
        assert!(gate.ensure_ffmpeg_allowed().is_ok());
        gate.begin_meeting().unwrap();
    }
    #[test]
    fn startup_ack_never_clears_unverified_or_live_process_fences() {
        let mut gate = CaptureGate::awaiting_restore();
        gate.note_unverified_ffmpeg();
        assert!(gate.finish_startup_restore().is_err());
        assert!(gate.begin_dictation().is_err());
        let mut owned = CaptureGate::awaiting_restore();
        owned.set_ffmpeg_open(true);
        owned.finish_startup_restore().unwrap();
        assert!(owned.begin_meeting().is_err());
        assert!(owned.ensure_cpal_allowed().is_err());
        owned.set_ffmpeg_open(false);
        assert!(owned.begin_meeting().is_ok());
    }
    #[test]
    fn startup_ack_does_not_release_an_existing_owner_or_shutdown() {
        let mut gate = CaptureGate::awaiting_restore();
        gate.finish_startup_restore().unwrap();
        let token = gate.begin_dictation().unwrap();
        gate.finish_startup_restore().unwrap();
        assert!(gate.begin_meeting().is_err());
        gate.end_dictation(&token).unwrap();
        gate.begin_shutdown();
        gate.finish_startup_restore().unwrap();
        assert!(gate.begin_dictation().is_err());
    }
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
