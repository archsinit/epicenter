//! Main-window IPC and Quit integration. Kept outside the portable meeting
//! module tree so core tests need no Tauri runtime or platform capture access.
use crate::meeting::controller::{ExitDecision, MeetingController};
use crate::meeting::request::StartMeetingParams;
use crate::meeting::sources::CaptureSources;
use crate::meeting::worker::{MeetingPhase, MeetingProgress};
use serde::{Deserialize, Serialize};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Manager, State, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

#[derive(Clone, Default)]
pub struct MeetingAppState {
    pub controller: Arc<MeetingController>,
    exit_task: Arc<AtomicBool>,
    exit_approved: Arc<AtomicBool>,
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
        let failure = match finalizer_state.controller.stop_blocking() {
            Ok(Some(progress)) if matches!(progress.phase, MeetingPhase::Interrupted | MeetingPhase::Failed) => Some(progress.error.unwrap_or_else(|| "Recording was marked incomplete".into())),
            Err(error) => Some(error.to_string()),
            _ => None,
        };
        let quit = if let Some(error) = failure {
            log::warn!("Meeting finalization could not fully complete: {error}");
            handle.dialog().message(format!("The meeting could not be fully finalized. Local audio files have been kept for recovery.\n\n{error}\n\nQuit anyway?"))
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
