//! Native meeting screenshot policy. The capture worker supplies monotonic elapsed
//! time, so hiding/reloading a webview cannot reset the five-minute cadence.

pub const SCREENSHOT_INTERVAL_MS: u64 = 5 * 60 * 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureOrigin {
    Automatic,
    Manual,
}

#[derive(Debug)]
pub struct ScreenshotSchedule {
    next_automatic_ms: u64,
    paused: bool,
    stopped: bool,
    in_flight: bool,
}

impl Default for ScreenshotSchedule {
    fn default() -> Self {
        Self {
            next_automatic_ms: SCREENSHOT_INTERVAL_MS,
            paused: false,
            stopped: false,
            in_flight: false,
        }
    }
}

impl ScreenshotSchedule {
    /// Reserve one capture. The owner must serialize this with finish/stop.
    /// Missed automatic slots are consumed even while paused or busy: never
    /// capture a burst of historical screenshots after sleep or unpausing.
    /// A manual capture does not consume an automatic slot.
    pub fn reserve(&mut self, elapsed_ms: u64, origin: CaptureOrigin) -> bool {
        if self.stopped {
            return false;
        }
        if origin == CaptureOrigin::Automatic {
            if elapsed_ms < self.next_automatic_ms {
                return false;
            }
            self.next_automatic_ms = elapsed_ms
                .checked_div(SCREENSHOT_INTERVAL_MS)
                .and_then(|slot| slot.checked_add(1))
                .and_then(|slot| slot.checked_mul(SCREENSHOT_INTERVAL_MS))
                .unwrap_or(u64::MAX);
        }
        if self.paused || self.in_flight {
            return false;
        }
        self.in_flight = true;
        true
    }

    /// Call for both successful and failed captures, after persistence finishes.
    pub fn finish_capture(&mut self) {
        self.in_flight = false;
    }

    /// Supply the current time when toggling pause; skip slots that elapsed
    /// during the pause instead of taking an immediate screenshot on resume.
    pub fn set_paused(&mut self, paused: bool, elapsed_ms: u64) {
        if self.paused && !paused && elapsed_ms >= self.next_automatic_ms {
            self.next_automatic_ms = elapsed_ms
                .checked_div(SCREENSHOT_INTERVAL_MS)
                .and_then(|slot| slot.checked_add(1))
                .and_then(|slot| slot.checked_mul(SCREENSHOT_INTERVAL_MS))
                .unwrap_or(u64::MAX);
        }
        self.paused = paused;
    }

    /// Prevent further reservations. False means the caller must drain the
    /// existing capture before finalizing the meeting manifest.
    pub fn stop(&mut self) -> bool {
        self.stopped = true;
        !self.in_flight
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_automatic_capture_is_at_five_minutes_not_startup() {
        let mut schedule = ScreenshotSchedule::default();
        assert!(!schedule.reserve(0, CaptureOrigin::Automatic));
        assert!(!schedule.reserve(299_999, CaptureOrigin::Automatic));
        assert!(schedule.reserve(300_000, CaptureOrigin::Automatic));
    }

    #[test]
    fn manual_capture_does_not_shift_automatic_cadence() {
        let mut schedule = ScreenshotSchedule::default();
        assert!(schedule.reserve(299_000, CaptureOrigin::Manual));
        schedule.finish_capture();
        assert!(schedule.reserve(300_000, CaptureOrigin::Automatic));
        schedule.finish_capture();
        assert!(!schedule.reserve(599_999, CaptureOrigin::Automatic));
        assert!(schedule.reserve(600_000, CaptureOrigin::Automatic));
    }

    #[test]
    fn no_overlapping_capture_or_duplicate_automatic_slot() {
        let mut schedule = ScreenshotSchedule::default();
        assert!(schedule.reserve(300_000, CaptureOrigin::Automatic));
        assert!(!schedule.reserve(300_001, CaptureOrigin::Manual));
        schedule.finish_capture();
        assert!(!schedule.reserve(300_002, CaptureOrigin::Automatic));
        assert!(schedule.reserve(300_003, CaptureOrigin::Manual));
    }

    #[test]
    fn sleep_skips_missed_slots_without_burst_or_cadence_drift() {
        let mut schedule = ScreenshotSchedule::default();
        assert!(schedule.reserve(950_000, CaptureOrigin::Automatic));
        schedule.finish_capture();
        assert!(!schedule.reserve(950_001, CaptureOrigin::Automatic));
        assert!(schedule.reserve(1_200_000, CaptureOrigin::Automatic));
    }

    #[test]
    fn paused_screenshots_include_manual_captures() {
        let mut schedule = ScreenshotSchedule::default();
        schedule.set_paused(true, 100_000);
        assert!(!schedule.reserve(300_000, CaptureOrigin::Automatic));
        assert!(!schedule.reserve(310_000, CaptureOrigin::Manual));
        schedule.set_paused(false, 650_000);
        assert!(!schedule.reserve(650_001, CaptureOrigin::Automatic));
        assert!(schedule.reserve(650_002, CaptureOrigin::Manual));
        schedule.finish_capture();
        assert!(schedule.reserve(900_000, CaptureOrigin::Automatic));
    }

    #[test]
    fn failed_capture_can_be_followed_by_manual_capture_without_retry_storm() {
        let mut schedule = ScreenshotSchedule::default();
        assert!(schedule.reserve(300_000, CaptureOrigin::Automatic));
        schedule.finish_capture(); // also called when backend fails
        assert!(!schedule.reserve(300_001, CaptureOrigin::Automatic));
        assert!(schedule.reserve(300_002, CaptureOrigin::Manual));
    }

    #[test]
    fn stop_requires_in_flight_capture_to_drain_and_is_permanent() {
        let mut schedule = ScreenshotSchedule::default();
        assert!(schedule.reserve(1, CaptureOrigin::Manual));
        assert!(!schedule.stop());
        assert!(!schedule.reserve(300_000, CaptureOrigin::Automatic));
        schedule.finish_capture();
        assert!(schedule.stop());
        schedule.set_paused(false, 600_000);
        assert!(!schedule.reserve(600_000, CaptureOrigin::Manual));
    }

    #[test]
    fn automatic_slot_missed_during_manual_capture_is_not_queued() {
        let mut schedule = ScreenshotSchedule::default();
        assert!(schedule.reserve(299_999, CaptureOrigin::Manual));
        assert!(!schedule.reserve(300_000, CaptureOrigin::Automatic));
        schedule.finish_capture();
        assert!(!schedule.reserve(300_001, CaptureOrigin::Automatic));
        assert!(schedule.reserve(600_000, CaptureOrigin::Automatic));
    }
}
