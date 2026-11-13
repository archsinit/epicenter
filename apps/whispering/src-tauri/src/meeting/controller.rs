//! Application-lifetime owner of a meeting worker. Lock order for short
//! operations is gate -> slot. Joining a worker happens with neither lock held,
//! so status remains readable and new capture is rejected during finalization.

use super::audio_recovery::{self, AudioRecoveryPlan, RecoveryPreview};
use super::capture_process::{CaptureProcessInfo, OwnedCaptureProcess};
use super::control::CaptureGate;
use super::storage::MeetingManifest;
use super::worker::{
    MeetingBackend, MeetingPhase, MeetingProgress, MeetingProgressReader, MeetingWorker,
};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static RECOVERY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveredAudioCopy {
    pub id: String,
    pub preview: RecoveryPreview,
}

#[derive(Default)]
struct Slot {
    worker: Option<MeetingWorker>,
    reader: Option<MeetingProgressReader>,
    last: Option<MeetingProgress>,
    failure: Option<String>,
    finishing: bool,
    maintenance: bool,
    maintenance_error: Option<String>,
    exit_pending: bool,
    exit_ready: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitDecision {
    Allow,
    Finalize,
    Wait,
}
#[derive(Default)]
pub struct MeetingController {
    gate: Mutex<CaptureGate>,
    slot: Mutex<Slot>,
    finalized: Condvar,
    ffmpeg: Mutex<Option<OwnedCaptureProcess>>,
}
fn poisoned() -> io::Error {
    io::Error::other("Capture state unavailable; no new recording was started")
}
impl MeetingController {
    pub fn awaiting_restore() -> Self {
        Self {
            gate: Mutex::new(CaptureGate::awaiting_restore()),
            ..Self::default()
        }
    }
    /// Explicit synchronous filesystem action, for a background caller only.
    /// Raw helper is not exposed by IPC; commands use confirmed transactions.
    /// The root must be native-owned;
    /// no webview chooses a path, output ID, source type or timing.
    pub fn recover_audio_copy(
        &self,
        root: &Path,
        source_id: &str,
    ) -> io::Result<RecoveredAudioCopy> {
        self.with_maintenance(|| {
            Self::reconstruct_recovery_plan(audio_recovery::inspect(root, source_id)?)
        })
    }

    pub(crate) fn prepare_audio_recovery(
        &self,
        root: &Path,
        source_id: &str,
    ) -> io::Result<AudioRecoveryPlan> {
        self.with_maintenance(|| audio_recovery::inspect(root, source_id))
    }

    pub(crate) fn execute_audio_recovery(
        &self,
        plan: AudioRecoveryPlan,
    ) -> io::Result<RecoveredAudioCopy> {
        self.with_maintenance(|| Self::reconstruct_recovery_plan(plan))
    }

    fn reconstruct_recovery_plan(plan: AudioRecoveryPlan) -> io::Result<RecoveredAudioCopy> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?;
        let id = format!(
            "recovered-{}-{}",
            now.as_nanos(),
            RECOVERY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let preview = plan.reconstruct(&id)?;
        Ok(RecoveredAudioCopy { id, preview })
    }

    /// No hardware or detached work may be created/returned by this operation.
    /// Only synchronous local filesystem work and plain metadata results. The
    /// gate/slot mutexes are never held across filesystem work.
    fn with_maintenance<T>(&self, operation: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
        let token = {
            let mut gate = self.capture_gate()?;
            let mut slot = self.slot.lock().map_err(|_| poisoned())?;
            Self::reap(&mut gate, &mut slot);
            if slot.worker.is_some() || slot.finishing || slot.maintenance || slot.failure.is_some()
            {
                return Err(io::Error::other(
                    "Native recording/finalization ownership is not idle or verified",
                ));
            }
            let token = gate.begin_maintenance()?;
            slot.maintenance = true;
            slot.maintenance_error = None;
            token
        };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
            .unwrap_or_else(|_| {
                Err(io::Error::other(
                    "Local audio recovery panicked; partial files were retained",
                ))
            });
        let release = (|| -> io::Result<()> {
            let mut gate = match self.capture_gate() {
                Ok(gate) => gate,
                Err(error) => {
                    if let Ok(mut slot) = self.slot.lock() {
                        slot.maintenance = false;
                        slot.failure = Some(error.to_string());
                    }
                    return Err(error);
                }
            };
            let mut slot = self.slot.lock().map_err(|_| poisoned())?;
            slot.maintenance = false;
            slot.maintenance_error = outcome.as_ref().err().map(|error| error.to_string());
            let released = gate.end_maintenance(&token);
            if let Err(error) = &released {
                slot.failure = Some(error.to_string());
            }
            released
        })();
        // Wake Quit even on poisoned admission/release failure, never label an
        // unknown gate released and never cancel shutdown on late completion.
        self.finalized.notify_all();
        release?;
        outcome
    }

    /// Filesystem failure is not capture failure. Quit checks this separately,
    /// AFTER stopping inputs; an older failed copy must never suppress Stop.
    pub fn maintenance_result(&self) -> io::Result<()> {
        let slot = self.slot.lock().map_err(|_| poisoned())?;
        match &slot.maintenance_error {
            Some(error) => Err(io::Error::other(error.clone())),
            None => Ok(()),
        }
    }

    pub fn spawn_ffmpeg(&self, command: &mut Command) -> io::Result<CaptureProcessInfo> {
        let mut gate = self.capture_gate()?;
        gate.ensure_ffmpeg_allowed()?;
        let mut process = self.ffmpeg.lock().map_err(|_| poisoned())?;
        if let Some(previous) = process.as_mut() {
            if previous.status()?.running {
                return Err(io::Error::other("A recording process is still active"));
            }
        }
        let mut owned = OwnedCaptureProcess::spawn(command)?;
        let info = owned.status()?;
        gate.set_ffmpeg_open(info.running);
        *process = Some(owned);
        Ok(info)
    }
    pub fn ffmpeg_status(&self, id: &str) -> io::Result<Option<CaptureProcessInfo>> {
        let mut process = self.ffmpeg.lock().map_err(|_| poisoned())?;
        match process.as_mut() {
            Some(process) if process.id() == id => process.status().map(Some),
            _ => Ok(None),
        }
    }
    pub fn ffmpeg_id(&self) -> io::Result<Option<String>> {
        Ok(self
            .ffmpeg
            .lock()
            .map_err(|_| poisoned())?
            .as_ref()
            .map(|process| process.id().to_owned()))
    }
    pub fn stop_ffmpeg(
        &self,
        id: &str,
        signal: impl FnOnce(u32) -> io::Result<()>,
        grace: Duration,
        force_wait: Duration,
    ) -> io::Result<CaptureProcessInfo> {
        let result = {
            let mut process = self.ffmpeg.lock().map_err(|_| poisoned())?;
            let process = process
                .as_mut()
                .filter(|process| process.id() == id)
                .ok_or_else(|| {
                    io::Error::other(
                        "Recording process is not owned by this session; no PID was signalled",
                    )
                })?;
            process.stop(signal, grace, force_wait)
        };
        // Never clear a newer child's fence after a concurrent/stale stop.
        let mut gate = self.capture_gate()?;
        let mut process = self.ffmpeg.lock().map_err(|_| poisoned())?;
        gate.set_ffmpeg_open(match process.as_mut() {
            Some(process) => process.status().map(|info| info.running).unwrap_or(true),
            None => false,
        });
        result
    }
    pub fn note_unverified_ffmpeg(&self) -> io::Result<()> {
        self.capture_gate()?.note_unverified_ffmpeg();
        Ok(())
    }
    pub fn capture_gate(&self) -> io::Result<MutexGuard<'_, CaptureGate>> {
        self.gate.lock().map_err(|_| poisoned())
    }
    fn record_finish(
        gate: &mut CaptureGate,
        slot: &mut Slot,
        outcome: io::Result<MeetingProgress>,
    ) -> io::Result<Option<MeetingProgress>> {
        let previous = slot.reader.as_ref().and_then(|reader| reader.read().ok());
        slot.reader = None;
        match outcome {
            Ok(mut progress) => {
                if !progress.cleanup_confirmed {
                    progress.capture_reserved = true;
                    let failure = progress.error.clone().unwrap_or_else(|| "Native input cleanup was not confirmed; fully Quit and relaunch before recording again".into());
                    slot.last = Some(progress);
                    slot.failure = Some(failure.clone());
                    return Err(io::Error::other(failure));
                }
                progress.capture_reserved = false;
                slot.last = Some(progress.clone());
                slot.failure = None;
                gate.end_meeting();
                Ok(Some(progress))
            }
            Err(error) => {
                slot.last = previous.map(|mut progress| {
                    progress.phase = MeetingPhase::Failed;
                    progress.error = Some(error.to_string());
                    progress
                });
                slot.failure = Some(error.to_string());
                // A panicked/unknown worker must not silently free capture
                // ownership. Local files remain available after an app restart.
                Err(error)
            }
        }
    }
    fn reap(gate: &mut CaptureGate, slot: &mut Slot) {
        if slot
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            let mut worker = slot.worker.take().unwrap();
            let _ = Self::record_finish(gate, slot, worker.finish());
        }
    }
    fn snapshot(slot: &Slot) -> io::Result<Option<MeetingProgress>> {
        if let Some(reader) = &slot.reader {
            return reader.read().map(Some);
        }
        if slot.last.is_none() {
            if let Some(failure) = &slot.failure {
                return Err(io::Error::other(format!(
                    "Capture ownership is unverified: {failure}"
                )));
            }
        }
        Ok(slot.last.clone())
    }
    pub fn start<B: MeetingBackend, F>(
        &self,
        root: PathBuf,
        manifest: MeetingManifest,
        factory: F,
    ) -> io::Result<MeetingProgress>
    where
        F: FnOnce(&MeetingManifest) -> io::Result<B> + Send + 'static,
    {
        let mut gate = self.capture_gate()?;
        let mut slot = self.slot.lock().map_err(|_| poisoned())?;
        Self::reap(&mut gate, &mut slot);
        if let Some(failure) = &slot.failure {
            return Err(io::Error::other(failure.clone()));
        }
        if slot.finishing || slot.maintenance || slot.worker.is_some() {
            return Err(io::Error::other(
                "A meeting is already active or finalizing",
            ));
        }
        gate.begin_meeting()?;
        let worker = match MeetingWorker::spawn(root, manifest, factory) {
            Ok(worker) => worker,
            Err(error) => {
                gate.end_meeting();
                return Err(error);
            }
        };
        // Transfer ownership before reading progress. A poisoned acknowledgement
        // must not drop a possibly live worker and leave the slot looking idle.
        slot.reader = Some(worker.reader());
        slot.worker = Some(worker);
        slot.last = None;
        Self::snapshot(&slot)?
            .ok_or_else(|| io::Error::other("Meeting startup identity unavailable"))
    }
    pub fn status(&self) -> io::Result<Option<MeetingProgress>> {
        let mut gate = self.capture_gate()?;
        let mut slot = self.slot.lock().map_err(|_| poisoned())?;
        Self::reap(&mut gate, &mut slot);
        Self::snapshot(&slot)
    }
    /// A terminal progress label alone is not proof the worker has been joined.
    /// Keep library playback fenced through finalization or unknown cleanup.
    pub fn active_meeting_id(&self) -> io::Result<Option<String>> {
        let mut gate = self.capture_gate()?;
        let mut slot = self.slot.lock().map_err(|_| poisoned())?;
        Self::reap(&mut gate, &mut slot);
        if slot.worker.is_some() || slot.finishing || slot.failure.is_some() {
            return Ok(Some(
                Self::snapshot(&slot)?
                    .ok_or_else(|| io::Error::other("Active meeting identity is unavailable"))?
                    .id,
            ));
        }
        Ok(None)
    }
    pub fn capture_now(&self) -> io::Result<()> {
        let gate = self.capture_gate()?;
        if gate.is_closing() {
            return Err(io::Error::other("Application is shutting down"));
        }
        let slot = self.slot.lock().map_err(|_| poisoned())?;
        slot.worker
            .as_ref()
            .ok_or_else(|| io::Error::other("No meeting is recording"))?
            .capture_now()
    }
    pub fn pause_screenshots(&self, paused: bool) -> io::Result<()> {
        let gate = self.capture_gate()?;
        if gate.is_closing() {
            return Err(io::Error::other("Application is shutting down"));
        }
        let slot = self.slot.lock().map_err(|_| poisoned())?;
        slot.worker
            .as_ref()
            .ok_or_else(|| io::Error::other("No meeting is recording"))?
            .pause_screenshots(paused)
    }
    pub fn request_stop(&self) -> io::Result<Option<MeetingProgress>> {
        let _gate = self.capture_gate()?;
        let slot = self.slot.lock().map_err(|_| poisoned())?;
        if let Some(worker) = &slot.worker {
            worker.request_stop()?;
        }
        Self::snapshot(&slot)
    }
    pub fn stop_blocking(&self) -> io::Result<Option<MeetingProgress>> {
        let mut worker = {
            let mut slot = self.slot.lock().map_err(|_| poisoned())?;
            while slot.finishing || slot.maintenance {
                slot = self.finalized.wait(slot).map_err(|_| poisoned())?;
            }
            if let Some(failure) = &slot.failure {
                return Err(io::Error::other(failure.clone()));
            }
            let Some(worker) = slot.worker.take() else {
                return Self::snapshot(&slot);
            };
            slot.finishing = true;
            worker
        };
        let outcome = worker.finish();
        let mut gate = match self.capture_gate() {
            Ok(gate) => gate,
            Err(error) => {
                if let Ok(mut slot) = self.slot.lock() {
                    slot.finishing = false;
                    slot.failure = Some(error.to_string());
                }
                self.finalized.notify_all();
                return Err(error);
            }
        };
        let mut slot = self.slot.lock().map_err(|_| poisoned())?;
        slot.finishing = false;
        let outcome = Self::record_finish(&mut gate, &mut slot, outcome);
        self.finalized.notify_all();
        outcome
    }
    /// Called for any Quit route (tray, Cmd-Q, plugin exit). It closes admission
    /// before deciding whether a background finalizer is needed.
    pub fn request_exit(&self) -> io::Result<ExitDecision> {
        let mut gate = self.capture_gate()?;
        let mut slot = self.slot.lock().map_err(|_| poisoned())?;
        if slot.exit_ready {
            return Ok(ExitDecision::Allow);
        }
        gate.begin_shutdown();
        Self::reap(&mut gate, &mut slot);
        // Completion alone does not resolve a prior Quit decision. A declined
        // timeout keeps admission closed, and a repeated Quit must return to
        // the monitor even if the worker has since finished.
        if slot.exit_pending {
            return Ok(ExitDecision::Wait);
        }
        if slot.worker.is_none()
            && !slot.finishing
            && !slot.maintenance
            && slot.maintenance_error.is_none()
            && slot.failure.is_none()
            && !gate.has_ffmpeg()
        {
            return Ok(ExitDecision::Allow);
        }
        slot.exit_pending = true;
        Ok(ExitDecision::Finalize)
    }
    pub fn resolve_exit(&self, quit: bool) -> io::Result<()> {
        let mut gate = self.capture_gate()?;
        let mut slot = self.slot.lock().map_err(|_| poisoned())?;
        slot.exit_pending = false;
        slot.exit_ready = quit;
        if !quit {
            slot.maintenance_error = None; // explicit acknowledgement of partial filesystem work
            gate.cancel_shutdown();
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "controller_tests.rs"]
mod tests;
