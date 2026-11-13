use super::super::storage::{MeetingMode, MeetingStatus, SavedMeeting};
use super::*;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Control {
    clock: AtomicU64,
    packets: Mutex<Vec<AudioPacket>>,
    stopped: AtomicBool,
    unhealthy: AtomicBool,
    shots: AtomicU64,
    screenshot_error: AtomicBool,
    gate: Mutex<Option<Receiver<()>>>,
}
impl Control {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            clock: AtomicU64::new(1_000_000),
            packets: Mutex::new(Vec::new()),
            stopped: AtomicBool::new(false),
            unhealthy: AtomicBool::new(false),
            shots: AtomicU64::new(0),
            screenshot_error: AtomicBool::new(false),
            gate: Mutex::new(None),
        })
    }
    fn packet(&self, source: AudioSource, ns: u64, samples: Vec<f32>) {
        self.packets.lock().unwrap().push(AudioPacket {
            source,
            host_time_ns: ns,
            samples,
        });
    }
}
struct FakeBackend(Arc<Control>);
impl MeetingBackend for FakeBackend {
    fn epoch_ns(&self) -> u64 {
        0
    }
    fn now_ns(&self) -> u64 {
        self.0.clock.load(Ordering::Acquire)
    }
    fn poll_audio(&mut self) -> io::Result<Vec<AudioPacket>> {
        Ok(std::mem::take(&mut *self.0.packets.lock().unwrap()))
    }
    fn check_health(&self) -> io::Result<()> {
        if self.0.unhealthy.load(Ordering::Acquire) {
            Err(io::Error::other("fixture packet loss"))
        } else {
            Ok(())
        }
    }
    fn stop(&mut self) -> io::Result<()> {
        self.0.stopped.store(true, Ordering::Release);
        Ok(())
    }
    fn screenshot_job(&self, _: u32) -> ScreenshotJob {
        let control = self.0.clone();
        let gate = control.gate.lock().unwrap().take();
        Box::new(move || {
            control.shots.fetch_add(1, Ordering::AcqRel);
            if let Some(gate) = gate {
                let _ = gate.recv_timeout(Duration::from_secs(5));
            }
            if control.screenshot_error.load(Ordering::Acquire) {
                return Err(io::Error::other("fixture screenshot failure"));
            }
            Ok(CapturedScreenshot {
                png: b"\x89PNG\r\n\x1a\nfixture".to_vec(),
                host_time_ns: control.clock.load(Ordering::Acquire),
            })
        })
    }
}
struct Fixture {
    root: PathBuf,
    control: Arc<Control>,
    engine: Option<Engine<FakeBackend>>,
}
fn manifest(mode: MeetingMode) -> MeetingManifest {
    MeetingManifest::new(
        "test".into(),
        "Fixture meeting".into(),
        mode,
        "fixture-mic".into(),
        Some(1),
        0,
    )
}
fn root() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "meeting-worker-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    path
}
fn wait_for(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "Fixture timed out");
        thread::sleep(Duration::from_millis(2));
    }
}
impl Fixture {
    fn new() -> Self {
        let root = root();
        let control = Control::new();
        let store = MeetingStore::create(
            &root.join("meetings"),
            manifest(MeetingMode::SystemAndMicrophone),
        )
        .unwrap();
        let progress = Arc::new(Mutex::new(MeetingProgress::new("test".into())));
        let engine = Engine::new(store, FakeBackend(control.clone()), progress).unwrap();
        Self {
            root,
            control,
            engine: Some(engine),
        }
    }
    fn engine(&mut self) -> &mut Engine<FakeBackend> {
        self.engine.as_mut().unwrap()
    }
    fn saved(&self) -> SavedMeeting {
        SavedMeeting::load(&self.root.join("meetings"), "test").unwrap()
    }
    fn block_screenshot(&self) -> mpsc::Sender<()> {
        let (sender, receiver) = mpsc::channel();
        *self.control.gate.lock().unwrap() = Some(receiver);
        sender
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(mut engine) = self.engine.take() {
            if let Some(task) = &engine.screenshot {
                task.cancelled.store(true, Ordering::Release);
            }
            let _ = engine.poll_screenshot(true);
            drop(engine);
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn stop_drains_both_inputs_preserves_sources_and_creates_audio_only_mix() {
    let mut fixture = Fixture::new();
    fixture
        .control
        .packet(AudioSource::Microphone, 0, vec![1.0; 48]);
    fixture
        .control
        .packet(AudioSource::System, 0, vec![0.5; 48]);
    fixture.engine().finish(None); // also drains packets queued at Stop
    assert!(fixture.control.stopped.load(Ordering::Acquire));
    assert_eq!(fixture.engine().progress.phase, MeetingPhase::Completed);
    assert_eq!(fixture.engine().progress.audio_chunks, 1);
    assert_eq!(fixture.engine().progress.source_chunks, 2);
    assert_eq!(fixture.saved().manifest().status, MeetingStatus::Completed);
    let saved = fixture.saved();
    let index = saved
        .manifest()
        .audio
        .iter()
        .position(|audio| audio.track == AudioTrack::Mixed)
        .unwrap();
    let samples = hound::WavReader::new(saved.open_audio_for_transcription(index).unwrap())
        .unwrap()
        .samples::<i16>()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(samples, vec![24575; 48]);
}

#[test]
fn slow_screenshot_does_not_block_audio_processing() {
    let mut fixture = Fixture::new();
    let gate = fixture.block_screenshot();
    fixture.engine().capture(CaptureOrigin::Manual).unwrap();
    wait_for(|| fixture.control.shots.load(Ordering::Acquire) == 1);
    fixture
        .control
        .packet(AudioSource::Microphone, 0, vec![0.25; 96]);
    fixture.control.clock.store(2_000_000, Ordering::Release);
    fixture.engine().tick().unwrap();
    fixture.engine().microphone.checkpoint().unwrap();
    let wav = fixture.root.join("meetings/test/audio/00000000.wav");
    assert_eq!(hound::WavReader::open(wav).unwrap().len(), 96);
    assert_eq!(fixture.engine().progress.phase, MeetingPhase::Recording);
    gate.send(()).unwrap();
    fixture.engine().poll_screenshot(true).unwrap();
    assert_eq!(fixture.saved().manifest().screenshots.len(), 1);
    fixture.engine().finish(None);
    assert_eq!(fixture.engine().progress.phase, MeetingPhase::Completed);
}

#[test]
fn pause_cancels_in_flight_png_and_resume_does_not_restore_it() {
    let mut fixture = Fixture::new();
    let gate = fixture.block_screenshot();
    fixture.engine().capture(CaptureOrigin::Manual).unwrap();
    wait_for(|| fixture.control.shots.load(Ordering::Acquire) == 1);
    fixture.engine().pause_screenshots(true);
    assert!(fixture.engine().capture(CaptureOrigin::Manual).is_err());
    fixture.engine().pause_screenshots(false);
    gate.send(()).unwrap();
    fixture.engine().poll_screenshot(true).unwrap();
    assert!(fixture.saved().manifest().screenshots.is_empty());
    assert_eq!(
        std::fs::read_dir(fixture.root.join("meetings/test/screenshots"))
            .unwrap()
            .count(),
        0
    );
    fixture.engine().capture(CaptureOrigin::Manual).unwrap();
    fixture.engine().poll_screenshot(true).unwrap();
    assert_eq!(fixture.saved().manifest().screenshots.len(), 1);
}

#[test]
fn screenshot_failure_is_visible_but_does_not_interrupt_audio() {
    let mut fixture = Fixture::new();
    fixture
        .control
        .screenshot_error
        .store(true, Ordering::Release);
    fixture.engine().capture(CaptureOrigin::Manual).unwrap();
    fixture.engine().poll_screenshot(true).unwrap();
    fixture.engine().tick().unwrap();
    assert_eq!(fixture.engine().progress.phase, MeetingPhase::Recording);
    assert!(fixture
        .engine()
        .progress
        .last_screenshot_error
        .as_ref()
        .unwrap()
        .contains("fixture screenshot failure"));
    fixture.engine().finish(None);
    assert_eq!(fixture.engine().progress.phase, MeetingPhase::Completed);
}

#[test]
fn stop_waits_for_cancelled_screenshot_without_extending_meeting_duration() {
    let mut fixture = Fixture::new();
    fixture
        .control
        .packet(AudioSource::Microphone, 0, vec![0.25; 48]);
    fixture.engine().tick().unwrap();
    let gate = fixture.block_screenshot();
    fixture.engine().capture(CaptureOrigin::Manual).unwrap();
    wait_for(|| fixture.control.shots.load(Ordering::Acquire) == 1);
    let mut engine = fixture.engine.take().unwrap();
    let stopped = thread::spawn(move || {
        engine.finish(None);
        engine.progress
    });
    wait_for(|| fixture.control.stopped.load(Ordering::Acquire));
    fixture
        .control
        .clock
        .store(30_000_000_000, Ordering::Release); // slow screenshot, not extra meeting audio
    assert!(!stopped.is_finished());
    gate.send(()).unwrap();
    let progress = stopped.join().unwrap();
    assert_eq!(progress.phase, MeetingPhase::Completed);
    assert_eq!(progress.elapsed_ms, 1);
    assert_eq!(progress.screenshots, 0);
    assert_eq!(fixture.saved().manifest().duration_ms, Some(1));
}

#[test]
fn health_failure_marks_incomplete_audio_instead_of_claiming_success() {
    let mut fixture = Fixture::new();
    fixture
        .control
        .packet(AudioSource::Microphone, 0, vec![0.25; 48]);
    fixture.control.unhealthy.store(true, Ordering::Release);
    let error = fixture.engine().tick().unwrap_err();
    fixture.engine().finish(Some(error.to_string()));
    assert_eq!(fixture.engine().progress.phase, MeetingPhase::Interrupted);
    assert_eq!(
        fixture.saved().manifest().status,
        MeetingStatus::Interrupted
    );
    assert!(fixture
        .engine()
        .progress
        .error
        .as_ref()
        .unwrap()
        .contains("fixture packet loss"));
    assert_eq!(fixture.engine().progress.source_chunks, 1);
}

#[test]
fn manual_capture_at_299_seconds_does_not_postpone_automatic_capture() {
    let mut fixture = Fixture::new();
    fixture
        .control
        .clock
        .store(299_000_000_000, Ordering::Release);
    fixture.engine().capture(CaptureOrigin::Manual).unwrap();
    fixture.engine().poll_screenshot(true).unwrap();
    fixture
        .control
        .clock
        .store(300_000_000_000, Ordering::Release);
    // Advance only screenshot policy here; a real microphone cannot jump five
    // minutes without PCM. That condition is independently rejected as a gap.
    fixture.engine().capture(CaptureOrigin::Automatic).unwrap();
    fixture.engine().poll_screenshot(true).unwrap();
    let saved = fixture.saved();
    assert_eq!(saved.manifest().screenshots.len(), 2);
    assert_eq!(
        saved.manifest().screenshots[0].origin,
        ScreenshotOrigin::Manual
    );
    assert_eq!(
        saved.manifest().screenshots[1].origin,
        ScreenshotOrigin::Automatic
    );
    assert_eq!(saved.manifest().screenshots[1].offset_ms, 300_000);
}

#[test]
fn actor_factory_and_shutdown_run_off_the_calling_thread() {
    let root = root();
    let control = Control::new();
    let backend_control = control.clone();
    control.packet(AudioSource::Microphone, 0, vec![0.25; 48]);
    let mut worker = MeetingWorker::spawn(
        root.join("meetings"),
        manifest(MeetingMode::Microphone),
        move |_| {
            assert_eq!(thread::current().name(), Some("meeting-audio-worker"));
            Ok(FakeBackend(backend_control))
        },
    )
    .unwrap();
    let progress = worker.finish().unwrap();
    assert_eq!(progress.phase, MeetingPhase::Completed);
    assert!(control.stopped.load(Ordering::Acquire));
    assert!(worker.capture_now().is_err());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn failed_start_preserves_an_interrupted_directory_without_capturing() {
    let root = root();
    let mut worker = MeetingWorker::spawn(
        root.join("meetings"),
        manifest(MeetingMode::Microphone),
        |_| -> io::Result<FakeBackend> { Err(io::Error::other("fixture permission denied")) },
    )
    .unwrap();
    let progress = worker.finish().unwrap();
    assert_eq!(progress.phase, MeetingPhase::Failed);
    assert!(progress
        .error
        .unwrap()
        .contains("fixture permission denied"));
    let saved = SavedMeeting::load(&root.join("meetings"), "test").unwrap();
    assert_eq!(saved.manifest().status, MeetingStatus::Interrupted);
    assert!(saved.manifest().audio.is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn screenshot_manifest_failure_does_not_stop_audio_or_claim_image_was_saved() {
    let mut fixture = Fixture::new();
    let path = fixture.root.join("meetings/test/meeting.json");
    let backup = fixture.root.join("meetings/test/fixture-backup.json");
    std::fs::rename(&path, &backup).unwrap();
    std::fs::create_dir(&path).unwrap(); // deterministic atomic-rename failure
    fixture.engine().capture(CaptureOrigin::Manual).unwrap();
    wait_for(|| {
        fixture
            .engine
            .as_ref()
            .unwrap()
            .screenshot
            .as_ref()
            .unwrap()
            .thread
            .is_finished()
    });
    fixture.engine().tick().unwrap();
    assert_eq!(fixture.engine().progress.phase, MeetingPhase::Recording);
    assert_eq!(fixture.engine().progress.screenshots, 0);
    assert!(fixture.engine().progress.last_screenshot_error.is_some());
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(&backup, &path).unwrap();
    fixture.engine().finish(None);
    assert_eq!(fixture.engine().progress.phase, MeetingPhase::Completed);
    assert!(fixture.saved().manifest().screenshots.is_empty());
}

#[test]
fn microphone_stall_is_reported_as_interrupted_not_a_successful_silent_meeting() {
    let mut fixture = Fixture::new();
    fixture
        .control
        .clock
        .store(11_000_000_001, Ordering::Release);
    let error = fixture.engine().tick().unwrap_err();
    assert!(error.to_string().contains("No microphone audio"));
    fixture.engine().finish(Some(error.to_string()));
    assert_eq!(fixture.engine().progress.phase, MeetingPhase::Interrupted);
    assert_eq!(
        fixture.saved().manifest().status,
        MeetingStatus::Interrupted
    );
}

#[test]
fn impossible_future_audio_timestamp_is_rejected_and_reported() {
    let mut fixture = Fixture::new();
    fixture
        .control
        .packet(AudioSource::Microphone, 3_000_000_000, vec![1.0; 48]);
    let error = fixture.engine().tick().unwrap_err();
    assert!(error.to_string().contains("outside the host clock"));
    fixture.engine().finish(Some(error.to_string()));
    assert_eq!(fixture.engine().progress.phase, MeetingPhase::Interrupted);
    assert_eq!(fixture.engine().progress.source_chunks, 0);
}
