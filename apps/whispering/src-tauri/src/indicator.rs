//! The indicator reads levels from the actual recording stream, never a second microphone.
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

static LEVEL: AtomicU32 = AtomicU32::new(0);
static FRAME_AT: AtomicU64 = AtomicU64::new(0);
static CLOCK: OnceLock<Instant> = OnceLock::new();

fn now_ms() -> u64 {
    CLOCK.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

pub fn reset_level() {
    LEVEL.store(0, Ordering::Relaxed);
    FRAME_AT.store(0, Ordering::Relaxed);
}

pub fn record_level(samples: impl Iterator<Item = f32>) {
    let (sum, count) = samples.fold((0.0_f32, 0_u32), |(sum, count), value| {
        (sum + value * value, count + 1)
    });
    let rms = if count > 0 {
        (sum / count as f32).sqrt()
    } else {
        0.0
    };
    LEVEL.store(rms.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    FRAME_AT.store(now_ms(), Ordering::Relaxed);
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    #[default]
    Idle,
    Preparing,
    Recording,
    Transcribing,
    Cleaning,
    Error,
}

#[derive(Default)]
pub struct IndicatorState(pub Mutex<(Phase, bool)>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    phase: Phase,
    level: Option<f32>,
    input_active: bool,
}

pub fn create_window(app: &tauri::App) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(
        app,
        "recording-indicator",
        WebviewUrl::App("recording-indicator".into()),
    )
    .title("Recording status")
    .inner_size(280.0, 64.0)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .focused(false)
    .focusable(false)
    .visible(false)
    .build()?;
    window.set_ignore_cursor_events(true)?;
    #[cfg(target_os = "macos")]
    window.set_visible_on_all_workspaces(true)?;
    if let Some(monitor) = window.primary_monitor()? {
        let size = monitor.size();
        let origin = monitor.position();
        let scale = monitor.scale_factor();
        window.set_position(tauri::PhysicalPosition::new(
            origin.x + (size.width as i32 - (280.0 * scale) as i32) / 2,
            origin.y + size.height as i32 - (120.0 * scale) as i32,
        ))?;
    }
    Ok(())
}

#[tauri::command]
pub fn set_indicator_state(
    phase: Phase,
    metered: bool,
    app: tauri::AppHandle,
    state: State<'_, IndicatorState>,
) -> Result<(), String> {
    *state.0.lock().map_err(|e| e.to_string())? = (phase, metered);
    let window = app
        .get_webview_window("recording-indicator")
        .ok_or("Recording indicator window unavailable")?;
    if phase == Phase::Idle {
        window.hide()
    } else {
        window.show()
    }
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_indicator_state(state: State<'_, IndicatorState>) -> Result<Snapshot, String> {
    let (phase, metered) = *state.0.lock().map_err(|e| e.to_string())?;
    let last_frame = FRAME_AT.load(Ordering::Relaxed);
    let input_active = last_frame > 0 && now_ms().saturating_sub(last_frame) < 500;
    Ok(Snapshot {
        phase,
        level: if metered && phase == Phase::Recording {
            Some(if input_active {
                f32::from_bits(LEVEL.load(Ordering::Relaxed))
            } else {
                0.0
            })
        } else {
            None
        },
        input_active,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metering_uses_real_samples_and_silence_is_zero() {
        record_level([0.5_f32, -0.5].into_iter());
        assert!((f32::from_bits(LEVEL.load(Ordering::Relaxed)) - 0.5).abs() < 0.001);
        record_level([0.0_f32; 32].into_iter());
        assert_eq!(f32::from_bits(LEVEL.load(Ordering::Relaxed)), 0.0);
    }
}
