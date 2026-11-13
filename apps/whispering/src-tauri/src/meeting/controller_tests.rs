use super::super::audio::{AudioPacket, AudioSource};
use super::super::storage::{MeetingMode, SavedMeeting};
use super::super::worker::ScreenshotJob;
use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Backend {
    sent: bool,
    stopped: Arc<AtomicBool>,
    gate: Option<std::sync::mpsc::Receiver<()>>,
}
impl MeetingBackend for Backend {
    fn epoch_ns(&self) -> u64 {
        0
    }
    fn now_ns(&self) -> u64 {
        1_000_000
    }
    fn poll_audio(&mut self) -> io::Result<Vec<AudioPacket>> {
        if self.sent {
            return Ok(Vec::new());
        }
        self.sent = true;
        Ok(vec![AudioPacket {
            source: AudioSource::Microphone,
            host_time_ns: 0,
            samples: vec![0.25; 48],
        }])
    }
    fn check_health(&self) -> io::Result<()> {
        Ok(())
    }
    fn stop(&mut self) -> io::Result<()> {
        self.stopped.store(true, Ordering::Release);
        if let Some(gate) = self.gate.take() {
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        Ok(())
    }
    fn screenshot_job(&self, _: u32) -> ScreenshotJob {
        Box::new(|| Err(io::Error::other("No screenshots in this fixture")))
    }
}
struct Fixture {
    root: PathBuf,
    controller: Arc<MeetingController>,
    stopped: Arc<AtomicBool>,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "meeting-controller-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self {
            root,
            controller: Arc::new(MeetingController::default()),
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }
    fn manifest() -> MeetingManifest {
        MeetingManifest::new(
            "test".into(),
            "test".into(),
            MeetingMode::Microphone,
            "fixture".into(),
            None,
            0,
        )
    }
    fn start(&self, gate: Option<std::sync::mpsc::Receiver<()>>) {
        let stopped = self.stopped.clone();
        self.controller
            .start(self.root.join("meetings"), Self::manifest(), move |_| {
                Ok(Backend {
                    sent: false,
                    stopped,
                    gate,
                })
            })
            .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Ok(Some(id)) = self.controller.ffmpeg_id() {
            let _ = self.controller.stop_ffmpeg(
                &id,
                interrupt_fixture,
                Duration::from_millis(100),
                Duration::from_secs(2),
            );
        }
        let _ = self.controller.stop_blocking();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
#[test]
fn startup_restore_blocks_worker_and_process_creation_but_not_status_or_quit() {
    let controller = MeetingController::awaiting_restore();
    assert!(controller.status().unwrap().is_none());
    let root = std::env::temp_dir().join(format!(
        "meeting-startup-pending-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = controller.start(
        root.clone(),
        Fixture::manifest(),
        |_| -> io::Result<Backend> {
            panic!("No backend may be constructed before startup restoration");
        },
    );
    assert!(result.is_err());
    assert!(!root.exists());
    let mut command = Command::new("/not-a-real-capture-program");
    let error = controller.spawn_ffmpeg(&mut command).unwrap_err();
    assert!(error.to_string().contains("startup restoration"));
    assert!(controller.ffmpeg_id().unwrap().is_none());
    assert_eq!(controller.request_exit().unwrap(), ExitDecision::Allow);
}

fn wait_for(mut ready: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(2));
    }
}
#[cfg(unix)]
fn interrupt_fixture(pid: u32) -> io::Result<()> {
    let status = std::process::Command::new("/bin/kill")
        .args(["-INT", &pid.to_string()])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("Fixture signal failed"))
    }
}
#[cfg(not(unix))]
fn interrupt_fixture(_: u32) -> io::Result<()> {
    Err(io::Error::other("Unix fixture only"))
}

#[cfg(unix)]
#[test]
fn owned_ffmpeg_fences_capture_and_lease_release_until_exit_is_confirmed() {
    let fixture = Fixture::new();
    let token = fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .unwrap();
    let child = fixture
        .controller
        .spawn_ffmpeg(std::process::Command::new("/bin/sleep").arg("30"))
        .unwrap();
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_meeting()
        .is_err());
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .ensure_cpal_allowed()
        .is_err());
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .end_dictation(&token)
        .is_err());
    assert_eq!(
        fixture.controller.request_exit().unwrap(),
        ExitDecision::Finalize
    );
    let stopped = fixture
        .controller
        .stop_ffmpeg(
            &child.capture_id,
            interrupt_fixture,
            Duration::from_millis(100),
            Duration::from_secs(2),
        )
        .unwrap();
    assert!(!stopped.running);
    fixture
        .controller
        .capture_gate()
        .unwrap()
        .end_dictation(&token)
        .unwrap();
    fixture.controller.resolve_exit(false).unwrap();
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_meeting()
        .is_ok());
}

#[cfg(unix)]
#[test]
fn stale_process_key_never_signals_or_releases_a_newer_child() {
    let fixture = Fixture::new();
    let first = fixture
        .controller
        .spawn_ffmpeg(std::process::Command::new("/bin/sleep").arg("30"))
        .unwrap();
    fixture
        .controller
        .stop_ffmpeg(
            &first.capture_id,
            interrupt_fixture,
            Duration::from_millis(100),
            Duration::from_secs(2),
        )
        .unwrap();
    let second = fixture
        .controller
        .spawn_ffmpeg(std::process::Command::new("/bin/sleep").arg("30"))
        .unwrap();
    assert_ne!(first.capture_id, second.capture_id);
    assert!(fixture
        .controller
        .stop_ffmpeg(
            &first.capture_id,
            |_| panic!("Must not signal another child"),
            Duration::ZERO,
            Duration::ZERO
        )
        .is_err());
    assert!(
        fixture
            .controller
            .ffmpeg_status(&second.capture_id)
            .unwrap()
            .unwrap()
            .running
    );
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_meeting()
        .is_err());
}

#[test]
fn unverified_legacy_session_is_not_treated_as_a_safe_capture_handoff() {
    let fixture = Fixture::new();
    fixture.controller.note_unverified_ffmpeg().unwrap();
    assert!(fixture.controller.ffmpeg_id().unwrap().is_none());
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_meeting()
        .is_err());
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .is_err());
    assert_eq!(
        fixture.controller.request_exit().unwrap(),
        ExitDecision::Finalize
    );
}

#[test]
fn startup_claims_gate_before_factory_and_stop_releases_only_after_join() {
    let fixture = Fixture::new();
    fixture.start(None);
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .is_err());
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .ensure_cpal_allowed()
        .is_err());
    let done = fixture.controller.stop_blocking().unwrap().unwrap();
    assert_eq!(done.phase, MeetingPhase::Completed);
    assert_eq!(done.audio_chunks, 1);
    assert!(fixture.stopped.load(Ordering::Acquire));
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .is_ok());
    assert!(SavedMeeting::load(&fixture.root.join("meetings"), "test").is_ok());
}
#[test]
fn busy_dictation_prevents_even_calling_the_meeting_factory() {
    let fixture = Fixture::new();
    let token = fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .unwrap();
    let called = Arc::new(AtomicBool::new(false));
    let factory_called = called.clone();
    let result = fixture.controller.start(
        fixture.root.join("meetings"),
        Fixture::manifest(),
        move |_| -> io::Result<Backend> {
            factory_called.store(true, Ordering::Release);
            Err(io::Error::other("Should not run"))
        },
    );
    assert!(result.is_err());
    assert!(!called.load(Ordering::Acquire));
    assert!(!fixture.root.join("meetings").exists());
    fixture
        .controller
        .capture_gate()
        .unwrap()
        .end_dictation(&token)
        .unwrap();
}
#[test]
fn status_stays_readable_and_new_capture_is_rejected_while_stopping() {
    let fixture = Fixture::new();
    let (release, gate) = std::sync::mpsc::channel();
    fixture.start(Some(gate));
    let controller = fixture.controller.clone();
    let first = thread::spawn(move || controller.stop_blocking().unwrap());
    wait_for(|| fixture.stopped.load(Ordering::Acquire));
    assert_eq!(
        fixture.controller.status().unwrap().unwrap().phase,
        MeetingPhase::Stopping
    );
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .is_err());
    let controller = fixture.controller.clone();
    let second = thread::spawn(move || controller.stop_blocking().unwrap());
    release.send(()).unwrap();
    assert_eq!(
        first.join().unwrap().unwrap().phase,
        MeetingPhase::Completed
    );
    assert_eq!(
        second.join().unwrap().unwrap().phase,
        MeetingPhase::Completed
    );
}
#[test]
fn every_quit_route_closes_admission_and_waits_for_one_finalizer() {
    let fixture = Fixture::new();
    fixture.start(None);
    assert_eq!(
        fixture.controller.request_exit().unwrap(),
        ExitDecision::Finalize
    );
    assert_eq!(
        fixture.controller.request_exit().unwrap(),
        ExitDecision::Wait
    );
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .is_err());
    fixture.controller.stop_blocking().unwrap();
    fixture.controller.resolve_exit(true).unwrap();
    assert_eq!(
        fixture.controller.request_exit().unwrap(),
        ExitDecision::Allow
    );
}
#[test]
fn idle_exit_does_not_create_worker_or_touch_recording_files() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.controller.request_exit().unwrap(),
        ExitDecision::Allow
    );
    assert!(!fixture.root.join("meetings").exists());
    assert!(fixture.controller.status().unwrap().is_none());
    fixture.controller.resolve_exit(false).unwrap();
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .is_ok());
}
#[test]
fn poisoned_admission_after_join_does_not_leave_finalizer_waiters_hanging() {
    let fixture = Fixture::new();
    fixture.start(None);
    let controller = fixture.controller.clone();
    assert!(thread::spawn(move || {
        let _guard = controller.capture_gate().unwrap();
        panic!("fixture admission state failure");
    })
    .join()
    .is_err());
    assert!(fixture.controller.stop_blocking().is_err());
    assert!(fixture.controller.stop_blocking().is_err());
    assert!(fixture.controller.capture_gate().is_err());
}

#[test]
fn failed_start_is_reaped_and_does_not_leave_a_permanent_capture_lease() {
    let fixture = Fixture::new();
    fixture
        .controller
        .start(
            fixture.root.join("meetings"),
            Fixture::manifest(),
            |_| -> io::Result<Backend> { Err(io::Error::other("fixture permission denied")) },
        )
        .unwrap();
    wait_for(|| {
        fixture
            .controller
            .status()
            .unwrap()
            .is_some_and(|progress| progress.phase == MeetingPhase::Failed)
    });
    wait_for(|| {
        fixture
            .controller
            .capture_gate()
            .unwrap()
            .begin_dictation()
            .is_ok()
    });
}
