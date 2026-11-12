use crate::mouse_policy::{self, MousePolicy};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::State;

#[derive(Default)]
struct Inner {
    policy: Mutex<MousePolicy>,
    started: Mutex<bool>,
    healthy: AtomicBool,
}

#[derive(Default)]
pub struct MouseState(Arc<Inner>);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    button: u32,
    command_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Action {
    command_id: String,
    state: &'static str,
}

#[tauri::command]
pub async fn configure_mouse_bindings(
    bindings: Vec<Binding>,
    app: tauri::AppHandle,
    state: State<'_, MouseState>,
) -> Result<(), String> {
    let validated = mouse_policy::validate(
        bindings
            .into_iter()
            .map(|binding| (binding.button, binding.command_id))
            .collect(),
    )?;
    let inner = state.0.clone();
    tokio::task::spawn_blocking(move || {
        let mut started = inner.started.lock().map_err(|e| e.to_string())?;
        if !validated.is_empty() && !*started {
            start_worker(inner.clone(), app)?;
            *started = true;
        }
        inner.policy.lock().map_err(|e| e.to_string())?.bindings = validated;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn mouse_shortcuts_active(state: State<'_, MouseState>) -> bool {
    state.0.healthy.load(Ordering::Relaxed)
}

#[cfg(not(target_os = "macos"))]
fn start_worker(_inner: Arc<Inner>, _app: tauri::AppHandle) -> Result<(), String> {
    Err("Global mouse shortcuts are supported only on macOS".into())
}

#[cfg(target_os = "macos")]
fn start_worker(inner: Arc<Inner>, app: tauri::AppHandle) -> Result<(), String> {
    use crate::mouse_policy::ButtonEvent;
    use core_foundation::{
        base::TCFType,
        runloop::{kCFRunLoopCommonModes, kCFRunLoopDefaultMode, CFRunLoop},
    };
    use core_graphics::event::{
        CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
        CallbackResult, EventField,
    };
    use std::{sync::mpsc, thread, time::Duration};
    use tauri::Emitter;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapIsEnabled(tap: *mut std::ffi::c_void) -> bool;
    }

    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    thread::Builder::new().name("mouse-shortcut-tap".into()).spawn(move || {
        let (actions_tx, actions_rx) = mpsc::channel::<Action>();
        // Emit outside the event tap: slow UI/IPC work must not stall system mouse input.
        let emitter = thread::Builder::new().name("mouse-shortcut-actions".into()).spawn(move || {
            for action in actions_rx {
                if let Err(error) = app.emit_to("main", "mouse-shortcut", action) {
                    log::warn!("Mouse shortcut delivery failed: {}", error);
                }
            }
        });
        if let Err(error) = emitter {
            let _ = ready_tx.send(Err(error.to_string()));
            return;
        }
        let callback_inner = inner.clone();
        let tap = CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default, // Active filter: ListenOnly would not suppress clicks.
            vec![CGEventType::OtherMouseDown, CGEventType::OtherMouseUp, CGEventType::OtherMouseDragged],
            move |_proxy, event_type, event| {
                if matches!(event_type, CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput) {
                    callback_inner.healthy.store(false, Ordering::Relaxed);
                    if let Ok(mut policy) = callback_inner.policy.lock() {
                        for (command_id, _) in policy.release_all() {
                            let _ = actions_tx.send(Action { command_id, state: "Released" });
                        }
                    }
                    return CallbackResult::Keep;
                }
                let kind = match event_type {
                    CGEventType::OtherMouseDown => ButtonEvent::Down,
                    CGEventType::OtherMouseUp => ButtonEvent::Up,
                    CGEventType::OtherMouseDragged => ButtonEvent::Drag,
                    _ => return CallbackResult::Keep,
                };
                let button = event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER) as u32;
                let Ok(mut policy) = callback_inner.policy.lock() else { return CallbackResult::Keep; };
                let (suppress, action) = policy.handle(button, kind);
                drop(policy);
                if let Some((command_id, pressed)) = action {
                    let _ = actions_tx.send(Action { command_id, state: if pressed { "Pressed" } else { "Released" } });
                }
                if suppress { CallbackResult::Drop } else { CallbackResult::Keep }
            },
        );
        let Ok(tap) = tap else {
            let _ = ready_tx.send(Err("Cannot intercept mouse buttons. Grant Accessibility and Input Monitoring access to Whispering Intel in macOS System Settings, then retry.".into()));
            return;
        };
        let Ok(source) = tap.mach_port().create_runloop_source(0) else {
            let _ = ready_tx.send(Err("Cannot create mouse event run loop".into()));
            return;
        };
        let runloop = CFRunLoop::get_current();
        runloop.add_source(&source, unsafe { kCFRunLoopCommonModes });
        tap.enable();
        inner.healthy.store(true, Ordering::Relaxed);
        let _ = ready_tx.send(Ok(()));
        loop {
            CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, Duration::from_millis(250), false);
            // Recover from OS timeout; permission revocation cannot be bypassed by enable().
            let enabled = unsafe { CGEventTapIsEnabled(tap.mach_port().as_concrete_TypeRef().cast()) };
            inner.healthy.store(enabled, Ordering::Relaxed);
            if !enabled { tap.enable(); }
        }
    }).map_err(|e| e.to_string())?;
    ready_rx
        .recv()
        .map_err(|_| "Mouse shortcut setup thread stopped before reporting readiness".to_string())?
}
