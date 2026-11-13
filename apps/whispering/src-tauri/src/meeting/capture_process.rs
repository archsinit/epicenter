//! An owned recording subprocess, never an arbitrary persisted PID. Stop only
//! acknowledges after try_wait reaps this exact child. A durable exit receipt
//! lets a later app launch recognize a previously finalized recording safely.
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io;
use std::path::Path;
use std::process::{Child, Command, ExitStatus};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureProcessInfo {
    pub capture_id: String,
    pub pid: u32,
    pub running: bool,
    pub forced: bool,
    pub exit_code: Option<i32>,
}
pub struct OwnedCaptureProcess {
    child: Child,
    info: CaptureProcessInfo,
}
impl OwnedCaptureProcess {
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?;
        let id = format!(
            "ffmpeg-{}-{}",
            now.as_nanos(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let child = command.spawn()?;
        let pid = child.id();
        Ok(Self {
            child,
            info: CaptureProcessInfo {
                capture_id: id,
                pid,
                running: true,
                forced: false,
                exit_code: None,
            },
        })
    }
    pub fn id(&self) -> &str {
        &self.info.capture_id
    }
    fn exited(&mut self, status: ExitStatus) {
        self.info.running = false;
        self.info.exit_code = status.code();
    }
    pub fn status(&mut self) -> io::Result<CaptureProcessInfo> {
        if self.info.running {
            if let Some(status) = self.child.try_wait()? {
                self.exited(status);
            }
        }
        Ok(self.info.clone())
    }
    /// Signal only after checking for natural exit. Permission/unknown signal
    /// failures return immediately and retain ownership, without a force-kill
    /// fallback. Force-kill is only for a successfully signalled unresponsive
    /// owned child, and is followed by another bounded confirmation/reap.
    pub fn stop(
        &mut self,
        signal: impl FnOnce(u32) -> io::Result<()>,
        grace: Duration,
        force_wait: Duration,
    ) -> io::Result<CaptureProcessInfo> {
        if !self.status()?.running {
            return Ok(self.info.clone());
        }
        if let Err(error) = signal(self.child.id()) {
            if !self.status()?.running {
                return Ok(self.info.clone());
            }
            return Err(error);
        }
        if self.wait_until(Instant::now() + grace)? {
            return Ok(self.info.clone());
        }
        self.child.kill()?;
        self.info.forced = true;
        if !self.wait_until(Instant::now() + force_wait)? {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Recording process did not confirm exit; capture remains reserved",
            ));
        }
        Ok(self.info.clone())
    }
    fn wait_until(&mut self, deadline: Instant) -> io::Result<bool> {
        loop {
            if !self.status()?.running {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
fn valid_id(id: &str) -> bool {
    id.starts_with("ffmpeg-")
        && id.len() <= 96
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
pub fn write_exit_receipt(root: &Path, info: &CaptureProcessInfo) -> io::Result<()> {
    if info.running || !valid_id(&info.capture_id) {
        return Err(io::Error::other(
            "Only confirmed owned-process exits can be persisted",
        ));
    }
    if !root.exists() {
        super::storage::private_directory(root)?;
    }
    super::storage::atomic_write(
        root,
        &format!("{}.json", info.capture_id),
        &serde_json::to_vec(info)?,
    )
}
pub fn read_exit_receipt(root: &Path, id: &str) -> io::Result<Option<CaptureProcessInfo>> {
    if !valid_id(id) {
        return Err(io::Error::other("Invalid recording process identifier"));
    }
    let path = root.join(format!("{id}.json"));
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if path.canonicalize()? != root.canonicalize()?.join(format!("{id}.json")) {
        return Err(io::Error::other(
            "Recording receipt must not be a symbolic link",
        ));
    }
    let info: CaptureProcessInfo = serde_json::from_reader(file)?;
    if info.capture_id != id || info.running {
        return Err(io::Error::other("Recording exit receipt is invalid"));
    }
    Ok(Some(info))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    fn sleeper() -> OwnedCaptureProcess {
        OwnedCaptureProcess::spawn(Command::new("/bin/sleep").arg("30")).unwrap()
    }
    fn signal(pid: u32, name: &str) -> io::Result<()> {
        let status = Command::new("/bin/kill")
            .args([name, &pid.to_string()])
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other("Fixture signal failed"))
        }
    }
    fn stop(process: &mut OwnedCaptureProcess) -> CaptureProcessInfo {
        process
            .stop(
                |pid| signal(pid, "-INT"),
                Duration::from_millis(100),
                Duration::from_secs(2),
            )
            .unwrap()
    }
    #[test]
    fn confirms_and_reaps_owned_child_and_is_idempotent_without_resignalling() {
        let mut process = sleeper();
        let before = process.status().unwrap();
        assert!(before.running);
        let after = stop(&mut process);
        assert!(!after.running);
        assert!(!after.forced);
        assert_eq!(before.capture_id, after.capture_id);
        assert!(
            !process
                .stop(
                    |_| panic!("Must not signal an exited/reused PID"),
                    Duration::ZERO,
                    Duration::ZERO
                )
                .unwrap()
                .running
        );
    }
    #[test]
    fn naturally_exited_process_needs_no_signal() {
        let mut process = OwnedCaptureProcess::spawn(&mut Command::new("/usr/bin/true")).unwrap();
        process.child.wait().unwrap();
        assert!(
            !process
                .stop(|_| panic!("Already exited"), Duration::ZERO, Duration::ZERO)
                .unwrap()
                .running
        );
    }
    #[test]
    fn permission_failure_never_escalates_to_kill_or_reports_stopped() {
        let mut process = sleeper();
        let error = process
            .stop(
                |_| {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "synthetic denial",
                    ))
                },
                Duration::ZERO,
                Duration::ZERO,
            )
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(process.status().unwrap().running);
        assert!(!process.info.forced);
        // Fixture-only cleanup: no actual OS permission was denied.
        process.child.kill().unwrap();
        process.child.wait().unwrap();
    }
    #[test]
    fn unresponsive_owned_child_is_reaped_after_force_stop() {
        use std::io::{BufRead, BufReader};
        use std::process::Stdio;
        // SIGSTOP + SIGINT did not reliably require force on Intel macOS.
        // Explicitly ignore INT and acknowledge the installed disposition
        // before signalling. exec preserves SIG_IGN and keeps the owned PID.
        let mut process = OwnedCaptureProcess::spawn(
            Command::new("/bin/sh")
                .args(["-c", "trap '' INT; printf 'ready\\n'; exec /bin/sleep 30"])
                .stdout(Stdio::piped()),
        )
        .unwrap();
        let mut line = String::new();
        BufReader::new(process.child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(line, "ready\n");
        let after = stop(&mut process);
        assert!(!after.running);
        assert!(after.forced);
    }
    #[test]
    fn receipt_survives_restart_without_signalling_a_persisted_pid() {
        let root = std::env::temp_dir().join(format!(
            "capture-receipts-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut process = sleeper();
        let active = process.status().unwrap();
        assert!(write_exit_receipt(&root, &active).is_err());
        let closed = stop(&mut process);
        write_exit_receipt(&root, &closed).unwrap();
        drop(process);
        let restored = read_exit_receipt(&root, &closed.capture_id)
            .unwrap()
            .unwrap();
        assert!(!restored.running);
        assert_eq!(restored.capture_id, closed.capture_id);
        assert!(read_exit_receipt(&root, "../../other").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
