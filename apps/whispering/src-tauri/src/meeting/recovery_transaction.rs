//! One native-owned review/operation, no automatic execution. Synchronous callers
//! run prepare/execute on a background task. No state lock spans filesystem work.
//! A ready plan is metadata only, holds no lease and may be discarded on Quit.
//! This state is intentionally NOT restored across process restart.
use super::audio_recovery::{AudioRecoveryPlan, RecoveryPreview};
use super::controller::{MeetingController, RecoveredAudioCopy};
use serde::Serialize;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryPhase {
    Preparing,
    Ready,
    Running,
    Completed,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryStatus {
    pub token: String,
    pub source_id: String,
    pub phase: RecoveryPhase,
    pub preview: Option<RecoveryPreview>,
    pub derived_id: Option<String>,
    pub error: Option<String>,
}
struct Review {
    status: RecoveryStatus,
    plan: Option<AudioRecoveryPlan>,
}
#[derive(Default)]
pub struct RecoveryTransactions {
    current: Mutex<Option<Review>>,
}
fn unavailable() -> io::Error {
    io::Error::other("Recovery transaction state unavailable; do not retry execution blindly")
}
impl RecoveryTransactions {
    pub fn status(&self) -> io::Result<Option<RecoveryStatus>> {
        Ok(self
            .current
            .lock()
            .map_err(|_| unavailable())?
            .as_ref()
            .map(|review| review.status.clone()))
    }

    pub fn prepare(
        &self,
        controller: &MeetingController,
        root: &Path,
        source_id: &str,
    ) -> io::Result<RecoveryStatus> {
        self.prepare_with(source_id, || {
            controller.prepare_audio_recovery(root, source_id)
        })
    }

    fn prepare_with(
        &self,
        source_id: &str,
        inspect: impl FnOnce() -> io::Result<AudioRecoveryPlan>,
    ) -> io::Result<RecoveryStatus> {
        if source_id.is_empty()
            || source_id.len() > 96
            || !source_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(io::Error::other("Invalid recovery source ID"));
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?;
        let token = format!(
            "review-{}-{}",
            now.as_nanos(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        {
            let mut current = self.current.lock().map_err(|_| unavailable())?;
            if current.as_ref().is_some_and(|review| {
                matches!(
                    review.status.phase,
                    RecoveryPhase::Preparing | RecoveryPhase::Running
                )
            }) {
                return Err(io::Error::other(
                    "A recovery operation is already preparing or running",
                ));
            }
            *current = Some(Review {
                status: RecoveryStatus {
                    token: token.clone(),
                    source_id: source_id.to_owned(),
                    phase: RecoveryPhase::Preparing,
                    preview: None,
                    derived_id: None,
                    error: None,
                },
                plan: None,
            });
        }
        let inspected = inspect();
        let mut current = self.current.lock().map_err(|_| unavailable())?;
        let review = current
            .as_mut()
            .filter(|review| review.status.token == token)
            .ok_or_else(unavailable)?;
        match inspected {
            Ok(plan) => {
                review.status.preview = Some(plan.preview().clone());
                review.status.phase = RecoveryPhase::Ready;
                review.plan = Some(plan);
            }
            Err(error) => {
                review.status.phase = RecoveryPhase::Failed;
                review.status.error = Some(error.to_string());
            }
        }
        Ok(review.status.clone())
    }

    /// Confirmed token only: duplicate pending or completed calls return cached
    /// status, never re-inspect/reconstruct, even after a lost acknowledgement.
    pub fn execute(
        &self,
        controller: &MeetingController,
        token: &str,
        confirmed: bool,
    ) -> io::Result<RecoveryStatus> {
        self.execute_with(token, confirmed, |plan| {
            controller.execute_audio_recovery(plan)
        })
    }

    fn execute_with(
        &self,
        token: &str,
        confirmed: bool,
        reconstruct: impl FnOnce(AudioRecoveryPlan) -> io::Result<RecoveredAudioCopy>,
    ) -> io::Result<RecoveryStatus> {
        if !confirmed {
            return Err(io::Error::other(
                "Explicit recovery confirmation is required",
            ));
        }
        let plan = {
            let mut current = self.current.lock().map_err(|_| unavailable())?;
            let review = current
                .as_mut()
                .filter(|review| review.status.token == token)
                .ok_or_else(|| {
                    io::Error::other("Recovery review expired or does not match; review again")
                })?;
            if review.status.phase != RecoveryPhase::Ready {
                return match review.status.phase {
                    RecoveryPhase::Running
                    | RecoveryPhase::Completed
                    | RecoveryPhase::Failed
                    | RecoveryPhase::Cancelled => Ok(review.status.clone()),
                    _ => Err(io::Error::other(
                        "Recovery review has not finished preparing",
                    )),
                };
            }
            let plan = review.plan.take().ok_or_else(unavailable)?;
            review.status.phase = RecoveryPhase::Running;
            plan
        };
        let outcome = reconstruct(plan);
        let mut current = self.current.lock().map_err(|_| unavailable())?;
        let review = current
            .as_mut()
            .filter(|review| review.status.token == token)
            .ok_or_else(unavailable)?;
        match outcome {
            Ok(copy) => {
                review.status.phase = RecoveryPhase::Completed;
                review.status.derived_id = Some(copy.id);
                review.status.preview = Some(copy.preview);
            }
            Err(error) => {
                review.status.phase = RecoveryPhase::Failed;
                review.status.error = Some(error.to_string());
            }
        }
        Ok(review.status.clone())
    }

    /// Ready review cancellation releases metadata only, not an executing job.
    pub fn cancel(&self, token: &str) -> io::Result<RecoveryStatus> {
        let mut current = self.current.lock().map_err(|_| unavailable())?;
        let review = current
            .as_mut()
            .filter(|review| review.status.token == token)
            .ok_or_else(|| io::Error::other("Recovery review does not match"))?;
        match review.status.phase {
            RecoveryPhase::Ready => {
                review.plan = None;
                review.status.phase = RecoveryPhase::Cancelled;
            }
            RecoveryPhase::Cancelled => {}
            _ => {
                return Err(io::Error::other(
                    "Only a ready recovery review can be cancelled; running work remains owned",
                ))
            }
        }
        Ok(review.status.clone())
    }
}

#[cfg(test)]
#[path = "recovery_transaction_tests.rs"]
mod tests;
