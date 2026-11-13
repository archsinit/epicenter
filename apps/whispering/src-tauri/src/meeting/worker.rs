//! Native meeting actor: audio draining never waits on screen capture or PNG
//! writes. No webview timers, provider calls, clipboard delivery or auto-start.

use super::audio::{AudioPacket, AudioSource};
use super::schedule::{CaptureOrigin, ScreenshotSchedule};
use super::storage::{
    AudioTrack, MeetingManifest, MeetingMode, MeetingStore, PendingScreenshot, ScreenshotOrigin,
};
use super::timeline::{mix_tracks, SourceTimeline};
use serde::Serialize;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct CapturedScreenshot {
    pub png: Vec<u8>,
    pub host_time_ns: u64,
}
pub type ScreenshotJob = Box<dyn FnOnce() -> io::Result<CapturedScreenshot> + Send + 'static>;

/// Construct the backend on the actor thread. Native audio ownership need not
/// be Send: all its methods and its destruction remain on that same thread.
pub trait MeetingBackend: 'static {
    fn epoch_ns(&self) -> u64;
    fn now_ns(&self) -> u64;
    fn poll_audio(&mut self) -> io::Result<Vec<AudioPacket>>;
    fn check_health(&self) -> io::Result<()>;
    /// Stop both sources, even if one fails. Remaining queued PCM is drainable.
    fn stop(&mut self) -> io::Result<()>;
    /// Data loss after drained Stop is not the same as unverified input closure.
    fn stopped_integrity(&self) -> io::Result<()> {
        Ok(())
    }
    fn screenshot_job(&self, display_id: u32) -> ScreenshotJob;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MeetingPhase {
    Starting,
    Recording,
    Stopping,
    Mixing,
    Completed,
    Interrupted,
    Failed,
}
impl MeetingPhase {
    fn terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Interrupted | Self::Failed)
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingProgress {
    pub id: String,
    pub title: String,
    pub mode: MeetingMode,
    pub microphone_id: String,
    pub display_id: Option<u32>,
    /// Only the application controller clears this after joining the worker.
    pub capture_reserved: bool,
    pub cleanup_confirmed: bool,
    pub phase: MeetingPhase,
    pub elapsed_ms: u64,
    pub audio_chunks: usize,
    pub source_chunks: usize,
    pub screenshots: usize,
    pub screenshots_paused: bool,
    pub last_screenshot_error: Option<String>,
    pub error: Option<String>,
}
impl MeetingProgress {
    fn new(manifest: &MeetingManifest) -> Self {
        Self {
            id: manifest.id.clone(),
            title: manifest.title.clone(),
            mode: manifest.mode.clone(),
            microphone_id: manifest.microphone_id.clone(),
            display_id: manifest.display_id,
            capture_reserved: true,
            cleanup_confirmed: true, // no backend has been constructed yet
            phase: MeetingPhase::Starting,
            elapsed_ms: 0,
            audio_chunks: 0,
            source_chunks: 0,
            screenshots: 0,
            screenshots_paused: false,
            last_screenshot_error: None,
            error: None,
        }
    }
}

enum Command {
    CaptureNow,
    PauseScreenshots(bool),
    Stop,
}
type SharedProgress = Arc<Mutex<MeetingProgress>>;

#[derive(Clone)]
pub struct MeetingProgressReader(SharedProgress);
impl MeetingProgressReader {
    pub fn read(&self) -> io::Result<MeetingProgress> {
        self.0
            .lock()
            .map(|state| state.clone())
            .map_err(|_| io::Error::other("Meeting worker state unavailable"))
    }
}

/// Own this in application state, not a page component. Dropping it stops and
/// joins the actor; closing/hiding a window must not drop application state.
pub struct MeetingWorker {
    commands: SyncSender<Command>,
    progress: SharedProgress,
    thread: Option<JoinHandle<()>>,
}
impl MeetingWorker {
    pub fn spawn<B: MeetingBackend, F>(
        root: PathBuf,
        manifest: MeetingManifest,
        factory: F,
    ) -> io::Result<Self>
    where
        F: FnOnce(&MeetingManifest) -> io::Result<B> + Send + 'static,
    {
        let progress = Arc::new(Mutex::new(MeetingProgress::new(&manifest)));
        let shared = progress.clone();
        let (commands, receiver) = mpsc::sync_channel(16);
        let thread = thread::Builder::new()
            .name("meeting-audio-worker".into())
            .spawn(move || {
                let result: io::Result<()> = (|| {
                    let mut store = MeetingStore::create(&root, manifest)?;
                    shared
                        .lock()
                        .map_err(|_| io::Error::other("Meeting state unavailable"))?
                        .cleanup_confirmed = false;
                    let backend = match factory(store.manifest()) {
                        Ok(backend) => backend,
                        Err(error) => {
                            let _ = store.finish(0, true);
                            return Err(error);
                        }
                    };
                    let mut engine = Engine::new(store, backend, shared.clone())?;
                    loop {
                        let command = match receiver.recv_timeout(Duration::from_millis(20)) {
                            Ok(command) => Some(command),
                            Err(mpsc::RecvTimeoutError::Timeout) => None,
                            Err(mpsc::RecvTimeoutError::Disconnected) => Some(Command::Stop),
                        };
                        match command {
                            Some(Command::Stop) => {
                                engine.finish(None);
                                return Ok(());
                            }
                            Some(Command::CaptureNow) => {
                                if let Err(error) = engine.capture(CaptureOrigin::Manual) {
                                    engine.progress.last_screenshot_error = Some(error.to_string());
                                }
                            }
                            Some(Command::PauseScreenshots(paused)) => {
                                engine.pause_screenshots(paused)
                            }
                            None => {}
                        }
                        if let Err(error) = engine.tick() {
                            engine.finish(Some(error.to_string()));
                            return Ok(());
                        }
                    }
                })();
                if let Err(error) = result {
                    if let Ok(mut state) = shared.lock() {
                        state.phase = MeetingPhase::Failed;
                        state.error = Some(error.to_string());
                    }
                }
            })?;
        Ok(Self {
            commands,
            progress,
            thread: Some(thread),
        })
    }
    pub fn reader(&self) -> MeetingProgressReader {
        MeetingProgressReader(self.progress.clone())
    }
    pub fn is_finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(|thread| thread.is_finished())
    }

    pub fn progress(&self) -> io::Result<MeetingProgress> {
        self.progress
            .lock()
            .map(|state| state.clone())
            .map_err(|_| io::Error::other("Meeting worker state unavailable"))
    }
    fn send(&self, command: Command) -> io::Result<()> {
        let phase = self.progress()?.phase;
        if matches!(command, Command::Stop) {
            if phase.terminal() || matches!(phase, MeetingPhase::Stopping | MeetingPhase::Mixing) {
                return Ok(());
            }
        } else if phase != MeetingPhase::Recording {
            return Err(io::Error::other(
                "Meeting is not ready for recording controls",
            ));
        }
        self.commands
            .try_send(command)
            .map_err(|_| io::Error::other("Meeting worker is busy or unavailable"))
    }
    pub fn capture_now(&self) -> io::Result<()> {
        self.send(Command::CaptureNow)
    }
    pub fn pause_screenshots(&self, paused: bool) -> io::Result<()> {
        self.send(Command::PauseScreenshots(paused))
    }
    pub fn request_stop(&self) -> io::Result<()> {
        if self.progress().is_ok_and(|p| {
            p.phase.terminal() || matches!(p.phase, MeetingPhase::Stopping | MeetingPhase::Mixing)
        }) {
            return Ok(());
        }
        // Progress corruption must never prevent a best-effort Stop message to
        // this owned worker. Closure is still unverified until finish/join.
        self.commands
            .try_send(Command::Stop)
            .map_err(|_| io::Error::other("Meeting worker is busy or unavailable"))
    }
    /// Blocking finalization. Invoke from an IPC blocking task, never AppKit's
    /// main thread. Stop drains audio and the pending screenshot before mixing.
    pub fn finish(&mut self) -> io::Result<MeetingProgress> {
        if let Some(thread) = self.thread.take() {
            let _ = self.commands.send(Command::Stop);
            thread.join().map_err(|_| {
                io::Error::other("Meeting worker panicked; local files are preserved")
            })?;
        }
        self.progress()
    }
}
impl Drop for MeetingWorker {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

struct CapturedFile {
    pending: PendingScreenshot,
    host_time_ns: u64,
    origin: ScreenshotOrigin,
}
struct ScreenshotTask {
    receiver: Receiver<io::Result<Option<CapturedFile>>>,
    thread: JoinHandle<()>,
    cancelled: Arc<AtomicBool>,
}
fn capture_to_file(
    job: ScreenshotJob,
    mut pending: PendingScreenshot,
    cancelled: &AtomicBool,
    origin: ScreenshotOrigin,
) -> io::Result<Option<CapturedFile>> {
    let image = match job() {
        Ok(image) => image,
        Err(error) => {
            let _ = pending.discard();
            return Err(error);
        }
    };
    if cancelled.load(Ordering::Acquire) {
        pending.discard()?;
        return Ok(None);
    }
    if let Err(error) = pending.write_png(&image.png) {
        let _ = pending.discard();
        return Err(error);
    }
    if cancelled.load(Ordering::Acquire) {
        pending.discard()?;
        return Ok(None);
    }
    Ok(Some(CapturedFile {
        pending,
        host_time_ns: image.host_time_ns,
        origin,
    }))
}

struct Engine<B: MeetingBackend> {
    backend: B,
    store: MeetingStore,
    microphone: SourceTimeline,
    system: SourceTimeline,
    schedule: ScreenshotSchedule,
    screenshot: Option<ScreenshotTask>,
    epoch_ns: u64,
    last_checkpoint_ns: u64,
    last_microphone_ns: u64,
    progress: MeetingProgress,
    shared: SharedProgress,
}
impl<B: MeetingBackend> Engine<B> {
    fn new(store: MeetingStore, backend: B, shared: SharedProgress) -> io::Result<Self> {
        let epoch_ns = backend.epoch_ns();
        if backend.now_ns() < epoch_ns {
            return Err(io::Error::other("Invalid audio host clock"));
        }
        let mut progress = MeetingProgress::new(store.manifest());
        progress.phase = MeetingPhase::Recording;
        progress.cleanup_confirmed = false;
        let engine = Self {
            backend,
            store,
            microphone: SourceTimeline::new(AudioSource::Microphone, epoch_ns),
            system: SourceTimeline::new(AudioSource::System, epoch_ns),
            schedule: ScreenshotSchedule::default(),
            screenshot: None,
            epoch_ns,
            last_checkpoint_ns: epoch_ns,
            last_microphone_ns: epoch_ns,
            progress,
            shared,
        };
        engine.publish();
        Ok(engine)
    }
    fn publish(&self) {
        if let Ok(mut state) = self.shared.lock() {
            *state = self.progress.clone();
        }
    }
    fn elapsed_ms(&self) -> u64 {
        self.backend.now_ns().saturating_sub(self.epoch_ns) / 1_000_000
    }
    fn counts(&mut self) {
        self.progress.audio_chunks = self
            .store
            .manifest()
            .audio
            .iter()
            .filter(|audio| audio.track == AudioTrack::Mixed)
            .count();
        self.progress.source_chunks =
            self.store.manifest().audio.len() - self.progress.audio_chunks;
        self.progress.screenshots = self.store.manifest().screenshots.len();
    }
    fn drain_audio(&mut self) -> io::Result<()> {
        for packet in self.backend.poll_audio()? {
            if packet.host_time_ns > self.backend.now_ns().saturating_add(2_000_000_000) {
                return Err(io::Error::other(
                    "Audio packet timestamp is outside the host clock",
                ));
            }
            match packet.source {
                AudioSource::Microphone => {
                    self.last_microphone_ns = self.last_microphone_ns.max(packet.host_time_ns);
                    self.microphone.push(packet, &mut self.store)?;
                }
                AudioSource::System => {
                    if self.store.manifest().mode == super::storage::MeetingMode::Microphone {
                        return Err(io::Error::other(
                            "Unexpected system audio in microphone-only mode",
                        ));
                    }
                    self.system.push(packet, &mut self.store)?;
                }
            }
        }
        Ok(())
    }
    fn tick(&mut self) -> io::Result<()> {
        self.drain_audio()?;
        if let Err(error) = self.poll_screenshot(false) {
            self.progress.last_screenshot_error = Some(error.to_string());
        }
        self.backend.check_health()?;
        let now = self.backend.now_ns();
        if now < self.epoch_ns {
            return Err(io::Error::other("Audio host clock moved backwards"));
        }
        if now.saturating_sub(self.last_microphone_ns) > 10_000_000_000 {
            return Err(io::Error::other(
                "No microphone audio received for ten seconds",
            ));
        }
        if now.saturating_sub(self.last_checkpoint_ns) >= 1_000_000_000 {
            self.microphone.checkpoint()?;
            self.system.checkpoint()?;
            self.last_checkpoint_ns = now;
        }
        if self.store.manifest().display_id.is_some() {
            if let Err(error) = self.capture(CaptureOrigin::Automatic) {
                self.progress.last_screenshot_error = Some(error.to_string());
            }
        }
        self.progress.elapsed_ms = self.elapsed_ms();
        self.counts();
        self.publish();
        Ok(())
    }
    fn pause_screenshots(&mut self, paused: bool) {
        self.schedule.set_paused(paused, self.elapsed_ms());
        self.progress.screenshots_paused = paused;
        if paused {
            if let Some(task) = &self.screenshot {
                task.cancelled.store(true, Ordering::Release);
            }
        }
        self.publish();
    }
    fn capture(&mut self, origin: CaptureOrigin) -> io::Result<()> {
        let display = self
            .store
            .manifest()
            .display_id
            .ok_or_else(|| io::Error::other("No screenshot display selected"))?;
        if !self.schedule.reserve(self.elapsed_ms(), origin) {
            return if origin == CaptureOrigin::Manual {
                Err(io::Error::other(
                    "Screenshots are paused or a capture is already in progress",
                ))
            } else {
                Ok(())
            };
        }
        let result = (|| {
            let pending = self.store.begin_screenshot()?;
            let job = self.backend.screenshot_job(display);
            let cancelled = Arc::new(AtomicBool::new(false));
            let cancel = cancelled.clone();
            let (sender, receiver) = mpsc::channel();
            let origin = match origin {
                CaptureOrigin::Automatic => ScreenshotOrigin::Automatic,
                CaptureOrigin::Manual => ScreenshotOrigin::Manual,
            };
            let thread = thread::Builder::new()
                .name("meeting-screenshot-worker".into())
                .spawn(move || {
                    let _ = sender.send(capture_to_file(job, pending, &cancel, origin));
                })?;
            Ok(ScreenshotTask {
                receiver,
                thread,
                cancelled,
            })
        })();
        match result {
            Ok(task) => self.screenshot = Some(task),
            Err(error) => {
                self.schedule.finish_capture();
                return Err(error);
            }
        }
        Ok(())
    }
    fn poll_screenshot(&mut self, wait: bool) -> io::Result<()> {
        let Some(task) = &self.screenshot else {
            return Ok(());
        };
        let result = if wait {
            task.receiver.recv().map_err(|_| ())
        } else {
            match task.receiver.try_recv() {
                Ok(result) => Ok(result),
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => Err(()),
            }
        };
        let task = self.screenshot.take().unwrap();
        let joined = task.thread.join();
        self.schedule.finish_capture();
        let result = result
            .unwrap_or_else(|_| Err(io::Error::other("Screenshot worker ended unexpectedly")));
        match result {
            Ok(Some(captured)) => {
                if task.cancelled.load(Ordering::Acquire) {
                    captured.pending.discard()?;
                } else if captured.host_time_ns < self.epoch_ns
                    || captured.host_time_ns > self.backend.now_ns()
                {
                    captured.pending.discard()?;
                    self.progress.last_screenshot_error =
                        Some("Invalid screenshot timestamp".into());
                } else {
                    self.store.commit_screenshot(
                        captured.pending,
                        (captured.host_time_ns - self.epoch_ns) / 1_000_000,
                        captured.origin,
                    )?;
                    self.progress.last_screenshot_error = None;
                }
            }
            Ok(None) => {}
            Err(error) => {
                // Cancellation normally returns Ok(None). Surface failures even
                // during cancellation, especially inability to remove a PNG.
                self.progress.last_screenshot_error = Some(error.to_string());
            }
        }
        if joined.is_err() {
            self.progress.last_screenshot_error = Some("Screenshot worker panicked".into());
        }
        Ok(())
    }
    fn finish(&mut self, initial_error: Option<String>) {
        let requested_duration_ms = self.elapsed_ms();
        self.progress.phase = MeetingPhase::Stopping;
        self.publish();
        self.schedule.stop();
        if let Some(task) = &self.screenshot {
            task.cancelled.store(true, Ordering::Release);
        }
        let mut errors = initial_error.into_iter().collect::<Vec<_>>();
        if let Err(error) = self.backend.check_health() {
            errors.push(error.to_string());
        }
        match self.backend.stop() {
            Ok(()) => self.progress.cleanup_confirmed = true,
            Err(error) => {
                self.progress.cleanup_confirmed = false;
                errors.push(format!("Native input closure is unverified: {error}"));
            }
        }
        if let Err(error) = self.backend.stopped_integrity() {
            errors.push(error.to_string());
        }
        let mut source_error = false;
        for result in [
            self.drain_audio(),
            self.microphone.finish(&mut self.store),
            self.system.finish(&mut self.store),
        ] {
            if let Err(error) = result {
                source_error = true;
                errors.push(error.to_string());
            }
        }
        if let Err(error) = self.poll_screenshot(true) {
            self.progress.last_screenshot_error = Some(error.to_string());
        }
        let asset_end = self
            .store
            .manifest()
            .audio
            .iter()
            .map(|audio| audio.offset_ms.saturating_add(audio.duration_ms))
            .chain(
                self.store
                    .manifest()
                    .screenshots
                    .iter()
                    .map(|image| image.offset_ms),
            )
            .max()
            .unwrap_or(0);
        let duration_ms = requested_duration_ms.max(asset_end);
        self.progress.elapsed_ms = duration_ms;
        self.progress.phase = MeetingPhase::Mixing;
        self.publish();
        if !source_error {
            if let Err(error) = mix_tracks(&mut self.store, duration_ms) {
                errors.push(error.to_string());
            }
        }
        let persisted = self.store.finish(duration_ms, !errors.is_empty());
        self.progress.phase = if persisted.is_err() {
            MeetingPhase::Failed
        } else if errors.is_empty() {
            MeetingPhase::Completed
        } else {
            MeetingPhase::Interrupted
        };
        if let Err(error) = persisted {
            errors.push(error.to_string());
        }
        self.progress.error = if errors.is_empty() {
            None
        } else {
            Some(errors.join("; "))
        };
        self.counts();
        self.publish();
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
