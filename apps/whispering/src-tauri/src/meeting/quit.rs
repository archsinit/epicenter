//! Bounded observation of one native finalizer. A timeout is NOT cancellation
//! or proof of input closure. This module cannot exit the app or release gates.
use std::io;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

pub type Completion = Result<(), String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    Complete(Completion),
    Pending,
}

#[derive(Default)]
pub struct FinalizationRun {
    result: Mutex<Option<Completion>>,
    changed: Condvar,
}
impl FinalizationRun {
    pub fn observe(&self, timeout: Duration) -> io::Result<Observation> {
        let result = self.result.lock().map_err(|_| poisoned())?;
        let (result, _) = self
            .changed
            .wait_timeout_while(result, timeout, |result| result.is_none())
            .map_err(|_| poisoned())?;
        Ok(match result.as_ref() {
            Some(result) => Observation::Complete(result.clone()),
            None => Observation::Pending,
        })
    }

    /// Only a completed job can no longer run delayed cleanup against a newer
    /// session. This does not itself prove capture closure: native gates do.
    pub fn is_complete(&self) -> io::Result<bool> {
        Ok(self.result.lock().map_err(|_| poisoned())?.is_some())
    }
}

#[derive(Default)]
pub struct QuitFinalizer {
    current: Mutex<Option<Arc<FinalizationRun>>>,
}
impl QuitFinalizer {
    /// Reuse pending work, never duplicate it after a declined timeout. A later
    /// explicit Quit needs a fresh job if the prior one has completed; its
    /// cached success must never authorize exiting a newer recording.
    pub fn start(
        &self,
        work: impl FnOnce() -> Completion + Send + 'static,
    ) -> io::Result<Arc<FinalizationRun>> {
        self.start_with(work, |job| {
            std::thread::Builder::new()
                .name("meeting-quit-cleanup".into())
                .spawn(job)
                .map(|_| ())
        })
    }

    fn start_with(
        &self,
        work: impl FnOnce() -> Completion + Send + 'static,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> io::Result<()>,
    ) -> io::Result<Arc<FinalizationRun>> {
        let mut current = self.current.lock().map_err(|_| poisoned())?;
        if let Some(run) = current.as_ref() {
            if !run.is_complete()? {
                return Ok(Arc::clone(run));
            }
        }
        let run = Arc::new(FinalizationRun::default());
        let worker_run = Arc::clone(&run);
        spawn(Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                .unwrap_or_else(|_| {
                    Err("Native finalization panicked; input closure is unverified".into())
                });
            if let Ok(mut slot) = worker_run.result.lock() {
                *slot = Some(result);
            }
            worker_run.changed.notify_all();
        }))?;
        *current = Some(Arc::clone(&run));
        Ok(run)
    }
}

fn poisoned() -> io::Error {
    io::Error::other("Quit finalizer state is unavailable; capture closure remains unverified")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };

    #[test]
    fn timeout_keeps_one_job_alive_and_retries_observe_its_late_result() {
        let manager = QuitFinalizer::default();
        let calls = Arc::new(AtomicUsize::new(0));
        assert!(manager.current.lock().unwrap().is_none());
        let (release, waiting) = mpsc::channel();
        let counted = Arc::clone(&calls);
        let run = manager
            .start(move || {
                counted.fetch_add(1, Ordering::SeqCst);
                waiting.recv_timeout(Duration::from_secs(5)).unwrap();
                Err("retained incomplete audio".into())
            })
            .unwrap();
        assert_eq!(
            run.observe(Duration::from_millis(2)).unwrap(),
            Observation::Pending
        );
        assert!(!run.is_complete().unwrap());
        for _ in 0..10 {
            let retry = manager.start(|| panic!("duplicate finalizer")).unwrap();
            assert!(Arc::ptr_eq(&run, &retry));
            assert_eq!(retry.observe(Duration::ZERO).unwrap(), Observation::Pending);
        }
        release.send(()).unwrap();
        let expected = Observation::Complete(Err("retained incomplete audio".into()));
        assert_eq!(run.observe(Duration::from_secs(5)).unwrap(), expected);
        assert_eq!(run.observe(Duration::ZERO).unwrap(), expected);
        assert!(run.is_complete().unwrap());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn completed_success_is_not_reused_to_authorize_a_newer_session() {
        let manager = QuitFinalizer::default();
        let first = manager.start(|| Ok(())).unwrap();
        assert_eq!(
            first.observe(Duration::from_secs(5)).unwrap(),
            Observation::Complete(Ok(()))
        );
        let (release, waiting) = mpsc::channel();
        let second = manager
            .start(move || {
                waiting.recv_timeout(Duration::from_secs(5)).unwrap();
                Err("newer session stop failed".into())
            })
            .unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(
            second.observe(Duration::ZERO).unwrap(),
            Observation::Pending
        );
        release.send(()).unwrap();
        assert_eq!(
            second.observe(Duration::from_secs(5)).unwrap(),
            Observation::Complete(Err("newer session stop failed".into()))
        );
    }

    #[test]
    fn panic_is_an_unverified_failure_not_success_or_permanent_wait() {
        let run = QuitFinalizer::default()
            .start(|| panic!("synthetic cleanup panic"))
            .unwrap();
        let Observation::Complete(Err(message)) = run.observe(Duration::from_secs(5)).unwrap()
        else {
            panic!("panic must be reported as failed completion");
        };
        assert!(message.contains("closure is unverified"));
    }

    #[test]
    fn failed_spawn_does_not_cache_a_job_or_execute_cleanup() {
        let manager = QuitFinalizer::default();
        assert!(manager
            .start_with(
                || panic!("must not run"),
                |_| Err(io::Error::other("synthetic spawn failure"))
            )
            .is_err());
        assert!(manager.current.lock().unwrap().is_none());
        let retry = manager.start(|| Ok(())).unwrap();
        assert_eq!(
            retry.observe(Duration::from_secs(5)).unwrap(),
            Observation::Complete(Ok(()))
        );
    }

    #[test]
    fn poisoned_state_is_an_error_never_an_idle_or_completed_claim() {
        let run = Arc::new(FinalizationRun::default());
        let poisoned = Arc::clone(&run);
        let _ = std::thread::spawn(move || {
            let _guard = poisoned.result.lock().unwrap();
            panic!("synthetic poisoned result");
        })
        .join();
        assert!(run.observe(Duration::ZERO).is_err());
        assert!(run.is_complete().is_err());
    }
}
