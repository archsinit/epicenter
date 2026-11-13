//! Main-window IPC and Quit integration. Kept outside the portable meeting
//! module tree so core tests need no Tauri runtime or platform capture access.
use crate::meeting::capture_process::{read_exit_receipt, write_exit_receipt, CaptureProcessInfo};
use crate::meeting::controller::{ExitDecision, MeetingController};
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
            controller.start(
                parent.join("meetings"),
                manifest,
                crate::meeting::backend_macos::MacMeetingBackend::new,
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
) -> Result<Option<MeetingProgress>> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || controller.status()).await
}
#[tauri::command]
pub async fn meeting_stop(
    window: WebviewWindow,
    state: State<'_, MeetingAppState>,
) -> Result<Option<MeetingProgress>> {
    main_window(&window)?;
    let controller = state.controller.clone();
    blocking(move || controller.request_stop()).await
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

/// Intercept every normal ExitRequested route, not only the tray menu. No work
/// is started on launch; with no meeting the existing exit behaviour is kept.
pub fn handle_exit(app: &tauri::AppHandle, api: &tauri::ExitRequestApi) {
    let state = app.state::<MeetingAppState>().inner().clone();
    if state.exit_approved.load(Ordering::Acquire) {
        return;
    }
    match state.controller.request_exit() {
        Ok(ExitDecision::Allow) => return,
        Ok(ExitDecision::Wait) => {
            api.prevent_exit();
            return;
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
    let spawned = std::thread::Builder::new().name("meeting-quit-finalizer".into()).spawn(move || {
        let process_failure = (|| -> io::Result<()> {
            if !finalizer_state.controller.capture_gate()?.has_ffmpeg() { return Ok(()); }
            if let Some(id) = finalizer_state.controller.ffmpeg_id()? {
                stop_owned_ffmpeg(&handle, &finalizer_state.controller, &id)?;
            }
            if finalizer_state.controller.capture_gate()?.has_ffmpeg() {
                return Err(io::Error::other("An unverified FFmpeg session remains; no unknown PID was signalled"));
            }
            Ok(())
        })().err().map(|error| error.to_string());
        let failure = match finalizer_state.controller.stop_blocking() {
            Ok(Some(progress)) if matches!(progress.phase, MeetingPhase::Interrupted | MeetingPhase::Failed) => Some(progress.error.unwrap_or_else(|| "Recording was marked incomplete".into())),
            Err(error) => Some(error.to_string()),
            _ => None,
        }.or(process_failure);
        let quit = if let Some(error) = failure {
            log::warn!("Meeting finalization could not fully complete: {error}");
            handle.dialog().message(format!("The recording could not be fully finalized. Local audio files have been kept for recovery. An input/process may still be active; keeping the app open is safer until its recorder is stopped.\n\n{error}\n\nQuit anyway?"))
                .title("Meeting recording")
                .buttons(MessageDialogButtons::OkCancelCustom("Quit anyway".into(), "Keep app open".into()))
                .blocking_show()
        } else { true };
        let _ = finalizer_state.controller.resolve_exit(quit);
        finalizer_state.exit_task.store(false, Ordering::Release);
        if quit {
            finalizer_state.exit_approved.store(true, Ordering::Release);
            handle.exit(0);
        } else if let Some(window) = handle.get_webview_window("main") { let _ = window.show(); }
    });
    if let Err(error) = spawned {
        state.exit_task.store(false, Ordering::Release);
        let _ = state.controller.resolve_exit(false);
        log::error!("Could not start meeting finalizer; application kept open: {error}");
    }
}
