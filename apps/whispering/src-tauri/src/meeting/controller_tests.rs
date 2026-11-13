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
    stop_error: bool,
    integrity_error: bool,
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
        if self.stop_error {
            return Err(io::Error::other("fixture input did not confirm closure"));
        }
        if let Some(gate) = self.gate.take() {
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        Ok(())
    }
    fn stopped_integrity(&self) -> io::Result<()> {
        if self.integrity_error {
            Err(io::Error::other("fixture final packet loss"))
        } else {
            Ok(())
        }
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
                    stop_error: false,
                    integrity_error: false,
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

#[test]
fn library_active_identity_survives_worker_handoff_until_finalization_joins() {
    let fixture = Fixture::new();
    assert_eq!(fixture.controller.active_meeting_id().unwrap(), None);
    let (release, waiting) = std::sync::mpsc::channel();
    fixture.start(Some(waiting));
    assert_eq!(
        fixture.controller.active_meeting_id().unwrap().as_deref(),
        Some("test")
    );
    let controller = fixture.controller.clone();
    let stop = thread::spawn(move || controller.stop_blocking().unwrap());
    wait_for(|| fixture.stopped.load(Ordering::Acquire));
    assert_eq!(
        fixture.controller.active_meeting_id().unwrap().as_deref(),
        Some("test")
    );
    release.send(()).unwrap();
    stop.join().unwrap();
    assert_eq!(fixture.controller.active_meeting_id().unwrap(), None);
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
    // Finishing after a monitor deadline must neither bypass the unresolved
    // user decision nor reopen admission. Repeated Quit re-enters monitoring.
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
    fixture.controller.resolve_exit(true).unwrap();
    assert_eq!(
        fixture.controller.request_exit().unwrap(),
        ExitDecision::Allow
    );
}
#[test]
fn timed_out_quit_keeps_admission_closed_even_after_late_native_completion() {
    use super::super::quit::{Observation, QuitFinalizer};
    let fixture = Fixture::new();
    let (release, waiting) = std::sync::mpsc::channel();
    fixture.start(Some(waiting));
    assert_eq!(
        fixture.controller.request_exit().unwrap(),
        ExitDecision::Finalize
    );
    let controller = Arc::clone(&fixture.controller);
    let jobs = QuitFinalizer::default();
    let run = jobs
        .start(move || {
            controller
                .stop_blocking()
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .unwrap();
    assert_eq!(run.observe(Duration::ZERO).unwrap(), Observation::Pending);
    assert_eq!(
        fixture.controller.request_exit().unwrap(),
        ExitDecision::Wait
    );
    assert!(Arc::ptr_eq(
        &run,
        &jobs.start(|| panic!("duplicate Stop")).unwrap()
    ));
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .is_err());
    release.send(()).unwrap();
    assert_eq!(
        run.observe(Duration::from_secs(5)).unwrap(),
        Observation::Complete(Ok(()))
    );
    assert!(fixture.controller.active_meeting_id().unwrap().is_none());
    // Late completion is not user approval or cancellation. It neither exits
    // nor lets a newer recorder race against delayed cleanup.
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
    // Simulate Keep-open chosen AFTER completion while the timeout dialog is
    // still visible: the monitor may now explicitly cancel shutdown admission.
    fixture.controller.resolve_exit(false).unwrap();
    let token = fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .unwrap();
    fixture
        .controller
        .capture_gate()
        .unwrap()
        .end_dictation(&token)
        .unwrap();
}

#[test]
fn maintenance_scope_blocks_capture_but_not_status_and_quit_waits_for_late_completion() {
    let fixture = Fixture::new();
    let (entered, observing) = std::sync::mpsc::channel();
    let (release, waiting) = std::sync::mpsc::channel();
    let controller = Arc::clone(&fixture.controller);
    let job = thread::spawn(move || {
        controller.with_maintenance(|| {
            entered.send(()).unwrap();
            waiting.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(42)
        })
    });
    observing.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(fixture.controller.status().unwrap().is_none());
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .is_err());
    assert!(fixture
        .controller
        .with_maintenance(|| -> io::Result<()> { panic!("duplicate job") })
        .is_err());
    let called = Arc::new(AtomicBool::new(false));
    let would_call = Arc::clone(&called);
    assert!(fixture
        .controller
        .start::<Backend, _>(
            fixture.root.join("meetings"),
            Fixture::manifest(),
            move |_| {
                would_call.store(true, Ordering::Release);
                panic!("must not open a backend");
            }
        )
        .is_err());
    assert!(!called.load(Ordering::Acquire));
    assert_eq!(
        fixture.controller.request_exit().unwrap(),
        ExitDecision::Finalize
    );
    let controller = Arc::clone(&fixture.controller);
    let finalizer = super::super::quit::QuitFinalizer::default();
    let run = finalizer
        .start(move || {
            controller
                .stop_blocking()
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .unwrap();
    assert_eq!(
        run.observe(Duration::ZERO).unwrap(),
        super::super::quit::Observation::Pending
    );
    release.send(()).unwrap();
    assert_eq!(job.join().unwrap().unwrap(), 42);
    assert_eq!(
        run.observe(Duration::from_secs(5)).unwrap(),
        super::super::quit::Observation::Complete(Ok(()))
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
    fixture.controller.resolve_exit(false).unwrap();
    let token = fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .unwrap();
    fixture
        .controller
        .capture_gate()
        .unwrap()
        .end_dictation(&token)
        .unwrap();
}
#[test]
fn filesystem_failure_or_panic_releases_only_maintenance_and_is_reported_for_quit() {
    for panic_job in [false, true] {
        let fixture = Fixture::new();
        let result: io::Result<()> = fixture.controller.with_maintenance(|| {
            if panic_job {
                panic!("synthetic filesystem panic");
            }
            Err(io::Error::other("synthetic filesystem failure"))
        });
        assert!(result.is_err());
        assert!(fixture.controller.maintenance_result().is_err());
        let token = fixture
            .controller
            .capture_gate()
            .unwrap()
            .begin_dictation()
            .unwrap();
        fixture
            .controller
            .capture_gate()
            .unwrap()
            .end_dictation(&token)
            .unwrap();
        // A previous failed copy must not prevent a later actual meeting Stop.
        fixture.start(None);
        assert!(
            fixture
                .controller
                .stop_blocking()
                .unwrap()
                .unwrap()
                .cleanup_confirmed
        );
        assert_eq!(
            fixture.controller.request_exit().unwrap(),
            ExitDecision::Finalize
        );
        fixture.controller.resolve_exit(false).unwrap();
        assert!(fixture.controller.maintenance_result().is_ok());
    }
}
#[test]
fn recovery_wrapper_has_native_id_and_never_opens_a_capture_factory() {
    let fixture = Fixture::new();
    fixture.start(None);
    fixture.controller.stop_blocking().unwrap();
    let source_before = std::fs::read(fixture.root.join("meetings/test/meeting.json")).unwrap();
    let result = fixture
        .controller
        .recover_audio_copy(&fixture.root.join("meetings"), "test")
        .unwrap();
    assert!(result.id.starts_with("recovered-"));
    assert_eq!(result.preview.source_id, "test");
    let saved = SavedMeeting::load(&fixture.root.join("meetings"), &result.id).unwrap();
    assert_eq!(
        saved.manifest().status,
        super::super::storage::MeetingStatus::Interrupted
    );
    assert!(saved.manifest().screenshots.is_empty());
    assert_eq!(
        std::fs::read(fixture.root.join("meetings/test/meeting.json")).unwrap(),
        source_before
    );
    assert!(fixture.controller.active_meeting_id().unwrap().is_none());
}
#[test]
fn startup_or_unknown_capture_rejects_maintenance_before_running_any_filesystem_work() {
    let startup = MeetingController::awaiting_restore();
    assert!(startup
        .with_maintenance(|| -> io::Result<()> {
            panic!("must not run");
        })
        .is_err());
    let fixture = Fixture::new();
    fixture.start(None);
    assert!(fixture
        .controller
        .with_maintenance(|| -> io::Result<()> {
            panic!("must not run");
        })
        .is_err());
    fixture.controller.stop_blocking().unwrap();
    fixture.controller.note_unverified_ffmpeg().unwrap();
    assert!(fixture
        .controller
        .with_maintenance(|| -> io::Result<()> {
            panic!("must not run");
        })
        .is_err());
}
#[test]
fn poisoned_gate_after_filesystem_work_wakes_quit_waiters_without_releasing_unknown_admission() {
    let fixture = Fixture::new();
    let controller = Arc::clone(&fixture.controller);
    let (finished, observing) = std::sync::mpsc::channel();
    let mut waiter = None;
    let result = fixture.controller.with_maintenance(|| {
        let stopping = Arc::clone(&controller);
        waiter = Some(thread::spawn(move || {
            let result = stopping.stop_blocking();
            finished.send(result.is_err()).unwrap();
        }));
        let poison = Arc::clone(&controller);
        let _ = thread::spawn(move || {
            let _gate = poison.capture_gate().unwrap();
            panic!("synthetic admission poison");
        })
        .join();
        Ok(())
    });
    assert!(result.is_err());
    assert!(!fixture.controller.slot.lock().unwrap().maintenance);
    assert!(observing.recv_timeout(Duration::from_secs(5)).unwrap());
    waiter.unwrap().join().unwrap();
    assert!(fixture.controller.capture_gate().is_err());
    assert!(fixture.controller.stop_blocking().is_err());
}
#[test]
fn unexpected_process_state_during_maintenance_is_not_silently_released() {
    let fixture = Fixture::new();
    let result = fixture.controller.with_maintenance(|| {
        fixture.controller.note_unverified_ffmpeg()?;
        Ok(())
    });
    assert!(result.is_err());
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .is_err());
    assert!(fixture.controller.stop_blocking().is_err());
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
fn unidentified_failure_is_not_reported_as_idle_or_a_successful_stop() {
    let fixture = Fixture::new();
    fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_meeting()
        .unwrap();
    fixture.controller.slot.lock().unwrap().failure = Some("unknown worker identity".into());
    assert!(fixture.controller.status().is_err());
    assert!(fixture.controller.request_stop().is_err());
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
fn failed_factory_without_cleanup_evidence_keeps_capture_reserved() {
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
    let progress = fixture.controller.status().unwrap().unwrap();
    assert!(progress.capture_reserved);
    assert!(!progress.cleanup_confirmed);
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .is_err());
    assert!(fixture.controller.stop_blocking().is_err());
}

#[test]
fn stop_failure_retains_capture_even_after_worker_join_but_metadata_is_available() {
    let fixture = Fixture::new();
    let stopped = fixture.stopped.clone();
    fixture
        .controller
        .start(
            fixture.root.join("meetings"),
            Fixture::manifest(),
            move |_| {
                Ok(Backend {
                    sent: false,
                    stopped,
                    gate: None,
                    stop_error: true,
                    integrity_error: false,
                })
            },
        )
        .unwrap();
    assert!(fixture.controller.stop_blocking().is_err());
    let progress = fixture.controller.status().unwrap().unwrap();
    assert!(progress.capture_reserved);
    assert!(!progress.cleanup_confirmed);
    assert_eq!(
        fixture.controller.active_meeting_id().unwrap().as_deref(),
        Some("test")
    );
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .is_err());
    assert!(fixture.root.join("meetings/test/meeting.json").is_file());
}

#[test]
fn integrity_loss_after_confirmed_stop_is_incomplete_but_not_unknown_capture() {
    let fixture = Fixture::new();
    let stopped = fixture.stopped.clone();
    let initial = fixture
        .controller
        .start(
            fixture.root.join("meetings"),
            Fixture::manifest(),
            move |_| {
                Ok(Backend {
                    sent: false,
                    stopped,
                    gate: None,
                    stop_error: false,
                    integrity_error: true,
                })
            },
        )
        .unwrap();
    assert!(initial.capture_reserved);
    assert_eq!(initial.title, "test");
    assert_eq!(initial.microphone_id, "fixture");
    let final_state = fixture.controller.stop_blocking().unwrap().unwrap();
    assert_eq!(final_state.phase, MeetingPhase::Interrupted);
    assert!(final_state.cleanup_confirmed);
    assert!(!final_state.capture_reserved);
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .is_ok());
}

#[test]
fn storage_failure_before_factory_proves_no_input_was_opened_and_releases_capture() {
    let fixture = Fixture::new();
    let blocked = fixture.root.join("not-a-directory");
    std::fs::write(&blocked, b"fixture").unwrap();
    fixture
        .controller
        .start(blocked, Fixture::manifest(), |_| -> io::Result<Backend> {
            panic!("Storage failed before any factory call");
        })
        .unwrap();
    wait_for(|| {
        fixture
            .controller
            .status()
            .unwrap()
            .is_some_and(|p| p.phase == MeetingPhase::Failed && !p.capture_reserved)
    });
    assert!(fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .is_ok());
}
