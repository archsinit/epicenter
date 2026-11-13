//! Main-window IPC and Quit integration. Kept outside the portable meeting
//! module tree so core tests need no Tauri runtime or platform capture access.
use crate::meeting::capture_process::{read_exit_receipt, write_exit_receipt, CaptureProcessInfo};
use crate::meeting::controller::{ExitDecision, MeetingController};
use crate::meeting::quit::{Completion, Observation, QuitFinalizer};
use crate::meeting::request::StartMeetingParams;
use crate::meeting::sources::CaptureSources;
use crate::meeting::worker::{MeetingPhase, MeetingProgress};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{Manager, State, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

#[derive(Clone)]
pub struct MeetingAppState {
    pub controller: Arc<MeetingController>,
    exit_task: Arc<AtomicBool>,
    quit_finalizer: Arc<QuitFinalizer>,
    exit_approved: Arc<AtomicBool>,
}
impl Default for MeetingAppState {
    fn default() -> Self {
        #[cfg(target_os = "macos")]
        let controller = MeetingController::awaiting_restore();
        #[cfg(not(target_os = "macos"))]
        let controller = MeetingController::default();
        Self {
            controller: Arc::new(controller),
            exit_task: Arc::new(AtomicBool::new(false)),
            quit_finalizer: Arc::new(QuitFinalizer::default()),
            exit_approved: Arc::new(AtomicBool::new(false)),
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(tag = "name")]
pub enum MeetingCommandError {
    MeetingCommandError { message: String },
}
impl From<io::Error> for MeetingCommandError {
    fn from(error: io::Error) -> Self {
        Self::MeetingCommandError {
            message: error.to_string(),
        }
    }
}
type Result<T> = std::result::Result<T, MeetingCommandError>;
fn main_window(window: &WebviewWindow) -> Result<()> {
    if window.label() != "main" {
        return Err(
            io::Error::other("Meeting controls are available only in the main window").into(),
        );
    }
    Ok(())
}
async fn blocking<T, F>(work: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> io::Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| MeetingCommandError::from(io::Error::other(error.to_string())))?
        .map_err(Into::into)
}
#[cfg(not(target_os = "macos"))]
fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::other(
        "Meeting capture requires macOS 14 or newer",
    ))
}

/// Called only after the main webview has awaited saved-session inspection.
/// Cleanup/restore commands deliberately remain usable while admission is shut.
#[tauri::command]
pub async fn capture_restore_ready(
    window: WebviewWindow,
    state: State<'_, MeetingAppState>,
) -> Result<()> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || controller.capture_gate()?.finish_startup_restore()).await
}

/// Explicit user-driven recovery, never startup cleanup. Does not reset the
/// gate or manufacture an exit receipt. Full Quit/relaunch remains required.
#[tauri::command]
pub async fn capture_recover_saved_session(
    window: WebviewWindow,
    saved_session: String,
    state: State<'_, MeetingAppState>,
    app: tauri::AppHandle,
) -> Result<Option<String>> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || {
        controller.capture_gate()?.ensure_metadata_recovery_allowed()?;
        crate::meeting::recovery::validate_saved_metadata(&saved_session)?;
        let confirmed = app.dialog()
            .message("Whispering cannot verify an earlier FFmpeg recording. It will NOT signal a saved PID or stop an unknown process.\n\nFirst stop the earlier recorder normally and independently verify it is no longer recording. If you cannot verify that, cancel.\n\nConfirm only if you have already stopped it. Its saved metadata will be archived locally and audio files kept untouched. This is your confirmation, not native proof of exit. Capture stays blocked until you fully Quit and relaunch Whispering Intel; closing the window is not enough.")
            .title("Recover saved recording metadata")
            .buttons(MessageDialogButtons::OkCancelCustom("I stopped it — archive metadata".into(), "Cancel".into()))
            .blocking_show();
        if !confirmed { return Ok(None); }
        // Recheck after the dialog: Quit or a known recorder may have changed
        // state. Hold the gate through persistence; never clear its quarantine.
        let gate = controller.capture_gate()?;
        gate.ensure_metadata_recovery_allowed()?;
        let parent = app.path().app_data_dir().map_err(|error| io::Error::other(error.to_string()))?;
        std::fs::create_dir_all(&parent)?;
        let path = crate::meeting::recovery::archive_confirmed_metadata(
            &parent.join("capture-recovery"), &saved_session,
        )?;
        Ok(Some(path.to_string_lossy().into_owned()))
    }).await
}

#[tauri::command]
pub async fn capture_acquire_dictation(
    window: WebviewWindow,
    state: State<'_, MeetingAppState>,
) -> Result<String> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || {
        controller.status()?;
        let mut gate = controller.capture_gate()?;
        gate.begin_dictation()
    })
    .await
}
#[tauri::command]
pub async fn capture_release_dictation(
    window: WebviewWindow,
    token: String,
    state: State<'_, MeetingAppState>,
) -> Result<()> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || {
        let mut gate = controller.capture_gate()?;
        gate.end_dictation(&token)
    })
    .await
}
#[tauri::command]
pub async fn meeting_sources(window: WebviewWindow) -> Result<CaptureSources> {
    main_window(&window)?;
    blocking(|| {
        #[cfg(target_os = "macos")]
        {
            crate::meeting::macos::sources()
        }
        #[cfg(not(target_os = "macos"))]
        {
            unsupported()
        }
    })
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MeetingPermission {
    Microphone,
    Screen,
}
#[tauri::command]
pub async fn meeting_request_permission(
    window: WebviewWindow,
    kind: MeetingPermission,
    state: State<'_, MeetingAppState>,
) -> Result<bool> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || {
        if controller.capture_gate()?.is_closing() {
            return Err(io::Error::other("Application is shutting down"));
        }
        #[cfg(target_os = "macos")]
        {
            Ok(match kind {
                MeetingPermission::Microphone => {
                    crate::meeting::macos::request_microphone_permission()
                }
                MeetingPermission::Screen => crate::meeting::macos::request_screen_permission(),
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = kind;
            unsupported()
        }
    })
    .await
}
#[tauri::command]
pub async fn meeting_start(
    window: WebviewWindow,
    params: StartMeetingParams,
    state: State<'_, MeetingAppState>,
    app: tauri::AppHandle,
) -> Result<MeetingProgress> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || {
        let manifest = params.into_manifest()?;
        #[cfg(target_os = "macos")]
        {
            let parent = app
                .path()
                .app_data_dir()
                .map_err(|error| io::Error::other(error.to_string()))?;
            std::fs::create_dir_all(&parent)?;
            crate::menu_bar::meeting_transition(
                &app,
                || {
                    controller.start(
                        parent.join("meetings"),
                        manifest,
                        crate::meeting::backend_macos::MacMeetingBackend::new,
                    )
                },
                || {
                    controller
                        .active_meeting_id()
                        .map(|id| id.is_some())
                        .unwrap_or(true)
                },
            )
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (manifest, controller, app);
            unsupported()
        }
    })
    .await
}
#[tauri::command]
pub async fn meeting_status(
    window: WebviewWindow,
    state: State<'_, MeetingAppState>,
    app: tauri::AppHandle,
) -> Result<Option<MeetingProgress>> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || {
        let result = controller.status();
        sync_meeting_mark(&app, &controller);
        result
    })
    .await
}
#[tauri::command]
pub async fn meeting_stop(
    window: WebviewWindow,
    state: State<'_, MeetingAppState>,
    app: tauri::AppHandle,
) -> Result<Option<MeetingProgress>> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || {
        crate::menu_bar::meeting_transition(
            &app,
            || controller.stop_blocking(),
            || {
                controller
                    .active_meeting_id()
                    .map(|id| id.is_some())
                    .unwrap_or(true)
            },
        )
    })
    .await
}
#[tauri::command]
pub async fn meeting_capture_now(
    window: WebviewWindow,
    state: State<'_, MeetingAppState>,
) -> Result<()> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || controller.capture_now()).await
}
#[tauri::command]
pub async fn meeting_pause_screenshots(
    window: WebviewWindow,
    paused: bool,
    state: State<'_, MeetingAppState>,
) -> Result<()> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || controller.pause_screenshots(paused)).await
}

fn sync_meeting_mark(app: &tauri::AppHandle, controller: &MeetingController) {
    if let Err(error) = crate::menu_bar::sync_meeting_recording(app, || {
        controller
            .active_meeting_id()
            .map(|id| id.is_some())
            .unwrap_or(true)
    }) {
        log::warn!("Native meeting status mark could not update: {error}");
    }
}

fn meeting_root(app: &tauri::AppHandle) -> io::Result<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| io::Error::other(error.to_string()))?
        .join("meetings"))
}
#[tauri::command]
pub async fn meeting_list_saved(
    window: WebviewWindow,
    state: State<'_, MeetingAppState>,
    app: tauri::AppHandle,
) -> Result<crate::meeting::library::MeetingLibrary> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || {
        crate::meeting::library::list(
            &meeting_root(&app)?,
            controller.active_meeting_id()?.as_deref(),
        )
    })
    .await
}
#[tauri::command]
pub async fn meeting_saved_detail(
    window: WebviewWindow,
    id: String,
    state: State<'_, MeetingAppState>,
    app: tauri::AppHandle,
) -> Result<crate::meeting::library::MeetingDetail> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || {
        crate::meeting::library::detail(
            &meeting_root(&app)?,
            &id,
            controller.active_meeting_id()?.as_deref(),
        )
    })
    .await
}
#[tauri::command]
pub async fn meeting_read_mixed_audio(
    window: WebviewWindow,
    id: String,
    index: usize,
    state: State<'_, MeetingAppState>,
    app: tauri::AppHandle,
) -> Result<tauri::ipc::Response> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || {
        crate::meeting::library::read_mixed_audio(
            &meeting_root(&app)?,
            &id,
            index,
            controller.active_meeting_id()?.as_deref(),
        )
        .map(tauri::ipc::Response::new)
    })
    .await
}
#[tauri::command]
pub async fn meeting_read_screenshot(
    window: WebviewWindow,
    id: String,
    index: usize,
    app: tauri::AppHandle,
) -> Result<tauri::ipc::Response> {
    main_window(&window)?;
    blocking(move || {
        crate::meeting::library::read_screenshot(&meeting_root(&app)?, &id, index)
            .map(tauri::ipc::Response::new)
    })
    .await
}

fn receipt_root(app: &tauri::AppHandle) -> io::Result<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| io::Error::other(error.to_string()))?
        .join("capture-process-exits"))
}
fn persist_process_exit(app: &tauri::AppHandle, info: &CaptureProcessInfo) -> io::Result<()> {
    let root = receipt_root(app)?;
    if let Some(parent) = root.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_exit_receipt(&root, info)
}
fn signal_owned_process(pid: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use nix::{
            errno::Errno,
            sys::signal::{kill, Signal},
            unistd::Pid,
        };
        match kill(
            Pid::from_raw(i32::try_from(pid).map_err(io::Error::other)?),
            Signal::SIGINT,
        ) {
            Ok(()) | Err(Errno::ESRCH) => Ok(()),
            Err(Errno::EPERM) => Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Permission denied stopping the owned recording process",
            )),
            Err(error) => Err(io::Error::other(error.to_string())),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Owned FFmpeg capture is supported on this fork's Mac target",
        ))
    }
}
fn stop_owned_ffmpeg(
    app: &tauri::AppHandle,
    controller: &MeetingController,
    id: &str,
) -> io::Result<CaptureProcessInfo> {
    if controller.ffmpeg_status(id)?.is_some() {
        let info = controller.stop_ffmpeg(
            id,
            signal_owned_process,
            Duration::from_secs(2),
            Duration::from_secs(2),
        )?;
        persist_process_exit(app, &info)?;
        return Ok(info);
    }
    if let Some(info) = read_exit_receipt(&receipt_root(app)?, id)? {
        return Ok(info);
    }
    controller.note_unverified_ffmpeg()?;
    Err(io::Error::other("The previous recording process cannot be verified. No saved PID was signalled. Stop its original recorder normally and keep the audio file for recovery."))
}
#[tauri::command]
pub async fn capture_ffmpeg_spawn(
    window: WebviewWindow,
    command: String,
    state: State<'_, MeetingAppState>,
    app: tauri::AppHandle,
) -> Result<CaptureProcessInfo> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || {
        let (program, args) = crate::command::parse_command(&command);
        if !matches!(Path::new(&program).file_name().and_then(|name| name.to_str()), Some("ffmpeg" | "ffmpeg.exe")) {
            return Err(io::Error::other("Recording process must be FFmpeg"));
        }
        let mut cmd = Command::new(program);
        cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::inherit());
        let info = controller.spawn_ffmpeg(&mut cmd)?;
        std::thread::sleep(Duration::from_millis(100));
        let status = controller.ffmpeg_status(&info.capture_id)?.ok_or_else(|| io::Error::other("Recording process ownership was lost"))?;
        if !status.running {
            stop_owned_ffmpeg(&app, &controller, &info.capture_id)?;
            return Err(io::Error::other("FFmpeg exited during startup. Check the device, permissions and recording options; any output file has been kept."));
        }
        Ok(status)
    }).await
}
#[tauri::command]
pub async fn capture_ffmpeg_stop(
    window: WebviewWindow,
    capture_id: String,
    state: State<'_, MeetingAppState>,
    app: tauri::AppHandle,
) -> Result<CaptureProcessInfo> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || stop_owned_ffmpeg(&app, &controller, &capture_id)).await
}
#[tauri::command]
pub async fn capture_ffmpeg_restore(
    window: WebviewWindow,
    capture_id: Option<String>,
    state: State<'_, MeetingAppState>,
    app: tauri::AppHandle,
) -> Result<CaptureProcessInfo> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || {
        if let Some(id) = capture_id {
            if let Some(info) = controller.ffmpeg_status(&id)? {
                if info.running { return Ok(info); }
                return stop_owned_ffmpeg(&app, &controller, &id);
            }
            match read_exit_receipt(&receipt_root(&app)?, &id) {
                Ok(Some(info)) => return Ok(info),
                Ok(None) => {},
                Err(error) => { controller.note_unverified_ffmpeg()?; return Err(error); }
            }
        }
        controller.note_unverified_ffmpeg()?;
        Err(io::Error::other("A previous FFmpeg session cannot be verified. No process was signalled and no recording file was deleted."))
    }).await
}

// A monitor panic must not permanently suppress later explicit Quit attempts.
// Dropping this guard never releases capture/shutdown admission.
struct ExitMonitorGuard(Arc<AtomicBool>);
impl Drop for ExitMonitorGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Intercept every normal ExitRequested route, not only the tray menu. No work
/// is started on launch; with no meeting the existing exit behaviour is kept.
pub fn handle_exit(app: &tauri::AppHandle, api: &tauri::ExitRequestApi) {
    let state = app.state::<MeetingAppState>().inner().clone();
    if state.exit_approved.load(Ordering::Acquire) {
        return;
    }
    // Do not mutate the exit decision while an earlier dialog/monitor is
    // still resolving. In particular, don't leave a new pending Quit behind
    // during the narrow Keep-open completion window.
    if state.exit_task.load(Ordering::Acquire) {
        api.prevent_exit();
        return;
    }
    match state.controller.request_exit() {
        Ok(ExitDecision::Allow) => return,
        Ok(ExitDecision::Wait) => {
            api.prevent_exit();
            // A previous timed-out monitor may have been dismissed. Repeated
            // Quit observes the same pending cleanup, not another Stop job.
        }
        Ok(ExitDecision::Finalize) => api.prevent_exit(),
        Err(error) => {
            log::warn!("Meeting exit state unavailable: {error}");
            api.prevent_exit();
        }
    }
    if state.exit_task.swap(true, Ordering::AcqRel) {
        return;
    }
    let handle = app.clone();
    let finalizer_state = state.clone();
    let spawned = std::thread::Builder::new().name("meeting-quit-monitor".into()).spawn(move || {
        let _monitor_guard = ExitMonitorGuard(Arc::clone(&finalizer_state.exit_task));
        let cleanup_handle = handle.clone();
        let cleanup_state = finalizer_state.clone();
        let run = finalizer_state.quit_finalizer.start(move || {
            finalize_native_capture(&cleanup_handle, &cleanup_state)
        });
        let observation = run.as_ref().map_err(|error| error.to_string())
            .and_then(|run| run.observe(Duration::from_secs(20)).map_err(|error| error.to_string()));
        let failure = match observation {
            Ok(Observation::Complete(Ok(()))) => None,
            Ok(Observation::Complete(Err(error))) | Err(error) => Some(error),
            Ok(Observation::Pending) => Some("Native finalization has not finished within 20 seconds. It may still be waiting for a driver or mixing audio. The timeout does not stop the worker or prove capture has stopped.".into()),
        };
        let quit = if let Some(error) = failure {
            log::warn!("Meeting finalization could not fully complete: {error}");
            handle.dialog().message(format!("The recording could not be fully finalized. Local audio files have been kept for recovery. An input/process may still be active; keeping the app open is safer until its recorder is stopped. If cleanup is still pending when you keep the app open, capture admission stays closed until full Quit/relaunch. Quit again to retry this confirmation; late completion will not close the app on its own.\n\n{error}\n\nQuit anyway?"))
                .title("Meeting recording")
                .buttons(MessageDialogButtons::OkCancelCustom("Quit anyway".into(), "Keep app open".into()))
                .blocking_show()
        } else { true };
        // Never reopen admission while delayed cleanup could still Stop a
        // newer session. No cleanup-thread callback resolves/approves Exit.
        let completed = run.as_ref().ok().is_some_and(|run| run.is_complete().unwrap_or(false));
        if quit || completed {
            let _ = finalizer_state.controller.resolve_exit(quit);
        }
        if quit {
            finalizer_state.exit_approved.store(true, Ordering::Release);
            handle.exit(0);
        } else if let Some(window) = handle.get_webview_window("main") { let _ = window.show(); }
    });
    if let Err(error) = spawned {
        state.exit_task.store(false, Ordering::Release);
        // An older pending cleanup may exist. Leave shutdown admission fenced.
        log::error!("Could not start Quit monitor; application kept open with capture admission closed: {error}");
    }
}

/// Cleanup only. In particular, late completion cannot show a dialog, exit the
/// application, cancel shutdown admission or clear an unverified input guard.
fn finalize_native_capture(handle: &tauri::AppHandle, state: &MeetingAppState) -> Completion {
    let process_failure = (|| -> io::Result<()> {
        if !state.controller.capture_gate()?.has_ffmpeg() {
            return Ok(());
        }
        if let Some(id) = state.controller.ffmpeg_id()? {
            stop_owned_ffmpeg(handle, &state.controller, &id)?;
        }
        if state.controller.capture_gate()?.has_ffmpeg() {
            return Err(io::Error::other(
                "An unverified FFmpeg session remains; no unknown PID was signalled",
            ));
        }
        Ok(())
    })()
    .err()
    .map(|error| error.to_string());
    let failure = match state.controller.stop_blocking() {
        Ok(Some(progress))
            if matches!(
                progress.phase,
                MeetingPhase::Interrupted | MeetingPhase::Failed
            ) =>
        {
            Some(
                progress
                    .error
                    .unwrap_or_else(|| "Recording was marked incomplete".into()),
            )
        }
        Err(error) => Some(error.to_string()),
        _ => None,
    }
    .or(process_failure);
    // Reflect verified closure even if Keep-open was chosen while the window
    // was hidden. This only updates the mark, never shutdown admission.
    sync_meeting_mark(handle, &state.controller);
    match failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
