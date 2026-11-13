use crate::recorder::recorder::{AudioRecording, RecorderState, Result};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;
use log::{debug, info};

/// Application state containing the recorder
pub struct AppData {
    pub recorder: Mutex<RecorderState>,
}

impl AppData {
    pub fn new() -> Self {
        Self {
            recorder: Mutex::new(RecorderState::new()),
        }
    }
}

#[tauri::command]
pub async fn enumerate_recording_devices(state: State<'_, AppData>) -> Result<Vec<String>> {
    debug!("Enumerating recording devices");
    let recorder = state
        .recorder
        .lock()
        .map_err(|e| format!("Failed to lock recorder: {}", e))?;
    recorder.enumerate_devices()
}

#[tauri::command]
pub async fn init_recording_session(
    device_identifier: String,
    recording_id: String,
    output_folder: String,
    sample_rate: Option<u32>,
    state: State<'_, AppData>,
    _app_handle: tauri::AppHandle,
    meetings: State<'_, crate::meeting_commands::MeetingAppState>,
) -> Result<()> {
    let mut capture = meetings.controller.capture_gate().map_err(|error| error.to_string())?;
    capture.ensure_cpal_allowed().map_err(|error| error.to_string())?;
    info!(
        "Initializing recording session: device={}, id={}, folder={}, sample_rate={:?}",
        device_identifier, recording_id, output_folder, sample_rate
    );

    // Use the provided output folder
    let recordings_dir = PathBuf::from(output_folder);

    // Create the directory if it doesn't exist
    if !recordings_dir.exists() {
        std::fs::create_dir_all(&recordings_dir)
            .map_err(|e| format!("Failed to create output folder: {}", e))?;
    }

    // Validate it's a directory (not a file)
    if !recordings_dir.is_dir() {
        return Err(format!(
            "Output path is not a directory: {:?}",
            recordings_dir
        ));
    }

    // Initialize the session with optional sample rate
    let mut recorder = state
        .recorder
        .lock()
        .map_err(|e| format!("Failed to lock recorder: {}", e))?;
    let result = recorder.init_session(device_identifier, recordings_dir, recording_id, sample_rate);
    capture.set_cpal_open(recorder.has_open_session());
    result
}

#[tauri::command]
pub async fn start_recording(state: State<'_, AppData>, meetings: State<'_, crate::meeting_commands::MeetingAppState>) -> Result<()> {
    let capture = meetings.controller.capture_gate().map_err(|error| error.to_string())?;
    capture.ensure_cpal_allowed().map_err(|error| error.to_string())?;
    info!("Starting recording");
    let mut recorder = state
        .recorder
        .lock()
        .map_err(|e| format!("Failed to lock recorder: {}", e))?;
    recorder.start_recording()
}

#[tauri::command]
pub async fn stop_recording(state: State<'_, AppData>) -> Result<AudioRecording> {
    info!("Stopping recording");
    let mut recorder = state
        .recorder
        .lock()
        .map_err(|e| format!("Failed to lock recorder: {}", e))?;
    recorder.stop_recording()
}

#[tauri::command]
pub async fn cancel_recording(state: State<'_, AppData>, meetings: State<'_, crate::meeting_commands::MeetingAppState>) -> Result<()> {
    let mut capture = meetings.controller.capture_gate().map_err(|error| error.to_string())?;
    info!("Cancelling recording");
    let mut recorder = state
        .recorder
        .lock()
        .map_err(|e| format!("Failed to lock recorder: {}", e))?;
    let result = recorder.cancel_recording();
    capture.set_cpal_open(recorder.has_open_session());
    result
}

#[tauri::command]
pub async fn close_recording_session(state: State<'_, AppData>, meetings: State<'_, crate::meeting_commands::MeetingAppState>) -> Result<()> {
    let mut capture = meetings.controller.capture_gate().map_err(|error| error.to_string())?;
    info!("Closing recording session");
    let mut recorder = state
        .recorder
        .lock()
        .map_err(|e| format!("Failed to lock recorder: {}", e))?;
    let result = recorder.close_session();
    capture.set_cpal_open(recorder.has_open_session());
    result
}

#[tauri::command]
pub async fn get_current_recording_id(state: State<'_, AppData>) -> Result<Option<String>> {
    debug!("Getting current recording ID");
    let recorder = state
        .recorder
        .lock()
        .map_err(|e| format!("Failed to lock recorder: {}", e))?;
    Ok(recorder.get_current_recording_id())
}
