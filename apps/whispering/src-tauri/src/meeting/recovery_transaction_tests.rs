use super::super::storage::{AudioTrack, MeetingManifest, MeetingMode, MeetingStore, SavedMeeting};
use super::*;
use std::fs;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

struct Fixture {
    root: std::path::PathBuf,
    controller: MeetingController,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "meeting-recovery-review-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let mut store = MeetingStore::create(
            &root,
            MeetingManifest::new(
                "source".into(),
                "Fixture".into(),
                MeetingMode::Microphone,
                "mic".into(),
                None,
                0,
            ),
        )
        .unwrap();
        let mut pending = store.begin_track_chunk(0, AudioTrack::Microphone).unwrap();
        let mut writer = hound::WavWriter::new(
            pending.writer_file().unwrap(),
            hound::WavSpec {
                channels: 1,
                sample_rate: 48_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        writer.write_sample(1000i16).unwrap();
        writer.finalize().unwrap();
        store.commit_audio_chunk(pending, 1).unwrap();
        Self {
            root,
            controller: MeetingController::default(),
        }
    }
    fn folders(&self) -> usize {
        fs::read_dir(&self.root).unwrap().count()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn review_and_status_are_read_only_and_execute_requires_explicit_matching_confirmation() {
    let fixture = Fixture::new();
    let transactions = RecoveryTransactions::default();
    assert!(transactions.status().unwrap().is_none());
    let original = fs::read(fixture.root.join("source/meeting.json")).unwrap();
    let ready = transactions
        .prepare(&fixture.controller, &fixture.root, "source")
        .unwrap();
    assert_eq!(ready.phase, RecoveryPhase::Ready);
    assert_eq!(ready.preview.as_ref().unwrap().source_id, "source");
    assert!(ready.token.starts_with("review-"));
    assert_eq!(fixture.folders(), 1);
    let dictation = fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .unwrap();
    fixture
        .controller
        .capture_gate()
        .unwrap()
        .end_dictation(&dictation)
        .unwrap();
    assert!(transactions
        .execute(&fixture.controller, &ready.token, false)
        .is_err());
    assert!(transactions
        .execute(&fixture.controller, "wrong", true)
        .is_err());
    assert_eq!(fixture.folders(), 1);
    let result = transactions
        .execute(&fixture.controller, &ready.token, true)
        .unwrap();
    assert_eq!(result.phase, RecoveryPhase::Completed);
    assert!(result
        .derived_id
        .as_ref()
        .unwrap()
        .starts_with("recovered-"));
    assert_eq!(fixture.folders(), 2);
    assert_eq!(
        fs::read(fixture.root.join("source/meeting.json")).unwrap(),
        original
    );
    let saved = SavedMeeting::load(&fixture.root, result.derived_id.as_ref().unwrap()).unwrap();
    assert!(saved.manifest().screenshots.is_empty());
    // Lost result/repeated execute cannot produce another derived directory.
    assert_eq!(
        transactions.status().unwrap().unwrap().derived_id,
        result.derived_id
    );
    assert_eq!(
        transactions
            .execute(&fixture.controller, &ready.token, true)
            .unwrap()
            .derived_id,
        result.derived_id
    );
    assert_eq!(fixture.folders(), 2);
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(!serialized.contains(fixture.root.to_str().unwrap()));
    assert!(!serialized.contains("audio/"));
    let later = transactions
        .prepare(&fixture.controller, &fixture.root, "source")
        .unwrap();
    assert_ne!(later.token, ready.token);
    assert!(transactions
        .execute(&fixture.controller, &ready.token, true)
        .is_err());
    assert_eq!(fixture.folders(), 2);
}

#[test]
fn ready_cancel_is_idempotent_and_new_review_invalidates_old_tokens_without_disk_output() {
    let fixture = Fixture::new();
    let transactions = RecoveryTransactions::default();
    let first = transactions
        .prepare(&fixture.controller, &fixture.root, "source")
        .unwrap();
    assert_eq!(
        transactions.cancel(&first.token).unwrap().phase,
        RecoveryPhase::Cancelled
    );
    assert_eq!(
        transactions.cancel(&first.token).unwrap().phase,
        RecoveryPhase::Cancelled
    );
    assert_eq!(
        transactions
            .execute(&fixture.controller, &first.token, true)
            .unwrap()
            .phase,
        RecoveryPhase::Cancelled
    );
    let second = transactions
        .prepare(&fixture.controller, &fixture.root, "source")
        .unwrap();
    assert_ne!(first.token, second.token);
    assert!(transactions
        .execute(&fixture.controller, &first.token, true)
        .is_err());
    assert!(transactions.cancel(&first.token).is_err());
    assert_eq!(fixture.folders(), 1);
    assert_eq!(
        fixture.controller.request_exit().unwrap(),
        super::super::controller::ExitDecision::Allow
    );
}

#[test]
fn capture_started_after_review_blocks_execution_without_stopping_it_or_retrying() {
    let fixture = Fixture::new();
    let transactions = RecoveryTransactions::default();
    let ready = transactions
        .prepare(&fixture.controller, &fixture.root, "source")
        .unwrap();
    let dictation = fixture
        .controller
        .capture_gate()
        .unwrap()
        .begin_dictation()
        .unwrap();
    let failed = transactions
        .execute(&fixture.controller, &ready.token, true)
        .unwrap();
    assert_eq!(failed.phase, RecoveryPhase::Failed);
    assert!(failed.error.is_some());
    fixture
        .controller
        .capture_gate()
        .unwrap()
        .end_dictation(&dictation)
        .unwrap();
    assert_eq!(
        transactions
            .execute(&fixture.controller, &ready.token, true)
            .unwrap()
            .phase,
        RecoveryPhase::Failed
    );
    assert_eq!(fixture.folders(), 1);
}

#[test]
fn duplicate_execute_observes_running_and_never_detaches_or_duplicates_work() {
    let fixture = Arc::new(Fixture::new());
    let transactions = Arc::new(RecoveryTransactions::default());
    let ready = transactions
        .prepare(&fixture.controller, &fixture.root, "source")
        .unwrap();
    let (entered, observing) = mpsc::channel();
    let (release, waiting) = mpsc::channel();
    let worker_fixture = Arc::clone(&fixture);
    let worker_transactions = Arc::clone(&transactions);
    let token = ready.token.clone();
    let worker = thread::spawn(move || {
        worker_transactions.execute_with(&token, true, |plan| {
            entered.send(()).unwrap();
            waiting.recv_timeout(Duration::from_secs(5)).unwrap();
            worker_fixture.controller.execute_audio_recovery(plan)
        })
    });
    observing.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        transactions.status().unwrap().unwrap().phase,
        RecoveryPhase::Running
    );
    assert_eq!(
        transactions
            .execute_with(&ready.token, true, |_| panic!("duplicate copy"))
            .unwrap()
            .phase,
        RecoveryPhase::Running
    );
    assert!(transactions.cancel(&ready.token).is_err());
    assert!(transactions
        .prepare(&fixture.controller, &fixture.root, "source")
        .is_err());
    release.send(()).unwrap();
    assert_eq!(
        worker.join().unwrap().unwrap().phase,
        RecoveryPhase::Completed
    );
    assert_eq!(fixture.folders(), 2);
}

#[test]
fn preparing_is_observable_and_rejects_execute_cancel_or_second_review() {
    let fixture = Arc::new(Fixture::new());
    let transactions = Arc::new(RecoveryTransactions::default());
    let (entered, observing) = mpsc::channel();
    let (release, waiting) = mpsc::channel();
    let worker_fixture = Arc::clone(&fixture);
    let worker_transactions = Arc::clone(&transactions);
    let worker = thread::spawn(move || {
        worker_transactions.prepare_with("source", || {
            entered.send(()).unwrap();
            waiting.recv_timeout(Duration::from_secs(5)).unwrap();
            worker_fixture
                .controller
                .prepare_audio_recovery(&worker_fixture.root, "source")
        })
    });
    observing.recv_timeout(Duration::from_secs(5)).unwrap();
    let preparing = transactions.status().unwrap().unwrap();
    assert_eq!(preparing.phase, RecoveryPhase::Preparing);
    assert!(transactions
        .execute(&fixture.controller, &preparing.token, true)
        .is_err());
    assert!(transactions.cancel(&preparing.token).is_err());
    assert!(transactions
        .prepare(&fixture.controller, &fixture.root, "source")
        .is_err());
    release.send(()).unwrap();
    assert_eq!(worker.join().unwrap().unwrap().phase, RecoveryPhase::Ready);
    assert_eq!(fixture.folders(), 1);
}

#[test]
fn stale_files_are_failed_cached_and_originals_not_repaired_implicitly() {
    use std::io::Write;
    let fixture = Fixture::new();
    let transactions = RecoveryTransactions::default();
    let ready = transactions
        .prepare(&fixture.controller, &fixture.root, "source")
        .unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(fixture.root.join("source/audio/00000000.wav"))
        .unwrap()
        .write_all(&[1, 2])
        .unwrap();
    let source = fs::read(fixture.root.join("source/audio/00000000.wav")).unwrap();
    let failed = transactions
        .execute(&fixture.controller, &ready.token, true)
        .unwrap();
    assert_eq!(failed.phase, RecoveryPhase::Failed);
    assert!(failed.error.as_ref().unwrap().contains("changed"));
    let count = fixture.folders();
    assert_eq!(
        transactions
            .execute(&fixture.controller, &ready.token, true)
            .unwrap()
            .phase,
        RecoveryPhase::Failed
    );
    assert_eq!(fixture.folders(), count);
    assert_eq!(
        fs::read(fixture.root.join("source/audio/00000000.wav")).unwrap(),
        source
    );
}

#[test]
fn rejected_inputs_or_startup_restore_never_write_or_prepare_a_runnable_plan() {
    let fixture = Fixture::new();
    let transactions = RecoveryTransactions::default();
    for id in ["", "../elsewhere", "file:///private", &"x".repeat(97)] {
        assert!(transactions
            .prepare(&fixture.controller, &fixture.root, id)
            .is_err());
    }
    assert!(transactions.status().unwrap().is_none());
    let startup = MeetingController::awaiting_restore();
    let failed = transactions
        .prepare(&startup, &fixture.root, "source")
        .unwrap();
    assert_eq!(failed.phase, RecoveryPhase::Failed);
    assert!(failed.preview.is_none());
    assert_eq!(
        transactions
            .execute(&startup, &failed.token, true)
            .unwrap()
            .phase,
        RecoveryPhase::Failed
    );
    assert_eq!(fixture.folders(), 1);
}

#[test]
fn fresh_native_process_does_not_restore_or_execute_a_saved_token() {
    let fixture = Fixture::new();
    let transactions = RecoveryTransactions::default();
    let ready = transactions
        .prepare(&fixture.controller, &fixture.root, "source")
        .unwrap();
    let fresh = RecoveryTransactions::default();
    assert!(fresh.status().unwrap().is_none());
    assert!(fresh
        .execute(&fixture.controller, &ready.token, true)
        .is_err());
    assert_eq!(fixture.folders(), 1);
}
