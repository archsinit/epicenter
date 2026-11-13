//! The indicator reads levels from the actual recording stream, never a second microphone.
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

static LEVEL: AtomicU32 = AtomicU32::new(0);
static FRAME_AT: AtomicU64 = AtomicU64::new(0);
static CLOCK: OnceLock<Instant> = OnceLock::new();

const WINDOW_WIDTH: f64 = 180.0;
const WINDOW_HEIGHT: f64 = 120.0;
const HEAD_CENTER_Y: f64 = 52.5;
const BOTTOM_MARGIN: f64 = 56.0;
static CURSOR_IGNORED: AtomicBool = AtomicBool::new(true);

fn pointer_hits_companion(phase: Phase, x: f64, y: f64) -> bool {
    let (rx, ry): (f64, f64) = if phase == Phase::Recording {
        (82.0, 24.0)
    } else {
        (55.0, 50.0)
    };
    ((x - WINDOW_WIDTH / 2.0) / rx).powi(2) + ((y - HEAD_CENTER_Y) / ry).powi(2) <= 1.0
}

fn update_cursor_mode(window: &tauri::WebviewWindow, ignore: bool) {
    let previous = CURSOR_IGNORED.swap(ignore, Ordering::Relaxed);
    if previous != ignore {
        #[cfg(target_os = "macos")]
        let result = crate::indicator_panel::ignore_mouse(window, ignore);
        #[cfg(not(target_os = "macos"))]
        let result = window
            .set_ignore_cursor_events(ignore)
            .map_err(|e| e.to_string());
        if let Err(error) = result {
            CURSOR_IGNORED.store(previous, Ordering::Relaxed);
            log::warn!("Could not update companion pointer mode: {}", error);
        }
    }
}

fn gaze_direction(dx: f64, dy: f64, scale: f64) -> [f64; 2] {
    let distance = 400.0 * scale.max(1.0);
    [
        (dx / distance).clamp(-1.0, 1.0),
        (dy / distance).clamp(-1.0, 1.0),
    ]
}

fn now_ms() -> u64 {
    CLOCK.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

pub fn reset_level() {
    LEVEL.store(0, Ordering::Relaxed);
    FRAME_AT.store(0, Ordering::Relaxed);
}

pub fn record_level(samples: impl Iterator<Item = f32>) {
    let rms = crate::audio_meter::rms(samples);
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
    gaze: [f64; 2],
    returning: bool,
}

pub fn create_window(app: &tauri::App) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(
        app,
        "recording-indicator",
        WebviewUrl::App("recording-indicator".into()),
    )
    .title("Recording status")
    .inner_size(WINDOW_WIDTH, WINDOW_HEIGHT)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .accept_first_mouse(true)
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
            origin.x + (size.width as i32 - (WINDOW_WIDTH * scale) as i32) / 2,
            origin.y + size.height as i32 - ((WINDOW_HEIGHT + BOTTOM_MARGIN) * scale) as i32,
        ))?;
    }
    // A real non-activating NSPanel hosts the existing content on macOS. The
    // Tauri NSWindow stays hidden as IPC owner; no runtime class replacement.
    #[cfg(target_os = "macos")]
    {
        crate::indicator_panel::create(&window).map_err(std::io::Error::other)?;
        crate::indicator_panel::show(&window).map_err(std::io::Error::other)?;
    }
    #[cfg(not(target_os = "macos"))]
    window.show()?;
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
    // Idle is now a persistent companion, not a hidden recording indicator.
    #[cfg(target_os = "macos")]
    {
        crate::indicator_panel::recording(&window, phase == Phase::Recording)?;
        return crate::indicator_panel::show(&window);
    }
    #[cfg(not(target_os = "macos"))]
    window.show().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_indicator_state(
    app: tauri::AppHandle,
    state: State<'_, IndicatorState>,
) -> Result<Snapshot, String> {
    let (phase, metered) = *state.0.lock().map_err(|e| e.to_string())?;
    let last_frame = FRAME_AT.load(Ordering::Relaxed);
    let input_active = last_frame > 0 && now_ms().saturating_sub(last_frame) < 500;
    // No global event hooks: use the pointer position to keep the transparent margins
    // click-through, enabling native primary-button dragging only over the character.
    // Window and pointer coordinates are both physical pixels, including monitor origins.
    let (gaze, returning) = app
        .get_webview_window("recording-indicator")
        .and_then(|window| {
            #[cfg(target_os = "macos")]
            let (x, y, scale, returning) = {
                let pointer = crate::indicator_panel::pointer(&window, phase == Phase::Recording)?;
                (pointer.x, pointer.y, pointer.scale, pointer.returning)
            };
            #[cfg(not(target_os = "macos"))]
            let (x, y, scale, returning) = {
                let cursor = window.cursor_position().ok()?;
                let origin = window.inner_position().ok()?;
                let scale = window.scale_factor().ok()?;
                (
                    (cursor.x - origin.x as f64) / scale,
                    (cursor.y - origin.y as f64) / scale,
                    scale,
                    false,
                )
            };
            update_cursor_mode(&window, !pointer_hits_companion(phase, x, y));
            Some((
                gaze_direction(
                    (x - WINDOW_WIDTH / 2.0) * scale,
                    (y - HEAD_CENTER_Y) * scale,
                    scale,
                ),
                returning,
            ))
        })
        .unwrap_or_else(|| {
            if let Some(window) = app.get_webview_window("recording-indicator") {
                update_cursor_mode(&window, true);
            }
            ([0.0, 0.0], false)
        });
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
        gaze,
        returning,
    })
}

#[tauri::command]
pub fn start_indicator_drag(window: tauri::WebviewWindow) -> Result<(), String> {
    if window.label() != "recording-indicator" {
        return Err("Only the companion window can initiate this drag".into());
    }
    #[cfg(target_os = "macos")]
    return crate::indicator_panel::drag(&window);
    #[cfg(not(target_os = "macos"))]
    window.start_dragging().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_character_regions_intercept_primary_drag() {
        for phase in [Phase::Idle, Phase::Transcribing, Phase::Recording] {
            assert!(pointer_hits_companion(phase, 90.0, HEAD_CENTER_Y));
            assert!(!pointer_hits_companion(phase, 0.0, 0.0));
            assert!(!pointer_hits_companion(phase, 179.0, 119.0));
        }
        assert!(pointer_hits_companion(
            Phase::Recording,
            160.0,
            HEAD_CENTER_Y
        ));
        assert!(!pointer_hits_companion(Phase::Idle, 160.0, HEAD_CENTER_Y));
        assert!(!pointer_hits_companion(Phase::Recording, 90.0, 90.0));
    }

    #[test]
    fn gaze_is_bounded_and_scale_independent() {
        assert_eq!(gaze_direction(0.0, 0.0, 1.0), [0.0, 0.0]);
        assert_eq!(gaze_direction(900.0, -900.0, 1.0), [1.0, -1.0]);
        assert_eq!(gaze_direction(200.0, -100.0, 1.0), [0.5, -0.25]);
        assert_eq!(gaze_direction(400.0, -200.0, 2.0), [0.5, -0.25]);
    }

    #[test]
    fn metering_uses_real_samples_and_silence_is_zero() {
        record_level([0.5_f32, -0.5].into_iter());
        assert!((f32::from_bits(LEVEL.load(Ordering::Relaxed)) - 0.5).abs() < 0.001);
        record_level([0.0_f32; 32].into_iter());
        assert_eq!(f32::from_bits(LEVEL.load(Ordering::Relaxed)), 0.0);
    }
}
