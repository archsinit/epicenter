//! A real non-activating NSPanel hosts the existing webview on macOS.
//! All AppKit FFI is serialized onto the UI thread. Tauri retains IPC ownership.
use std::sync::mpsc;
use tauri::WebviewWindow;

#[repr(C)]
#[derive(Default)]
pub struct Pointer {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
}

extern "C" {
    fn wi_companion_panel_create(window: *mut std::ffi::c_void) -> bool;
    fn wi_companion_panel_show() -> bool;
    fn wi_companion_panel_pointer(result: *mut Pointer, recording: bool) -> bool;
    fn wi_companion_panel_ignore_mouse(ignore: bool) -> bool;
    fn wi_companion_panel_drag() -> bool;
}

fn on_main<T: Send + 'static>(
    window: &WebviewWindow,
    action: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = mpsc::sync_channel(1);
    // Tauri executes inline when already on the UI thread, avoiding a self-deadlock.
    window
        .run_on_main_thread(move || {
            let _ = tx.send(action());
        })
        .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())
}

pub fn create(window: &WebviewWindow) -> Result<(), String> {
    let pointer = window.ns_window().map_err(|e| e.to_string())? as usize;
    let ok = on_main(window, move || unsafe {
        wi_companion_panel_create(pointer as *mut _)
    })?;
    if ok {
        Ok(())
    } else {
        Err("Could not create non-activating companion panel".into())
    }
}

pub fn show(window: &WebviewWindow) -> Result<(), String> {
    if on_main(window, || unsafe { wi_companion_panel_show() })? {
        Ok(())
    } else {
        Err("Companion panel unavailable".into())
    }
}

pub fn pointer(window: &WebviewWindow, recording: bool) -> Option<Pointer> {
    on_main(window, move || {
        let mut result = Pointer::default();
        if unsafe { wi_companion_panel_pointer(&mut result, recording) } {
            Some(result)
        } else {
            None
        }
    })
    .ok()
    .flatten()
}

pub fn ignore_mouse(window: &WebviewWindow, ignore: bool) -> Result<(), String> {
    if on_main(window, move || unsafe {
        wi_companion_panel_ignore_mouse(ignore)
    })? {
        Ok(())
    } else {
        Err("Could not set companion pointer mode".into())
    }
}

pub fn drag(window: &WebviewWindow) -> Result<(), String> {
    if on_main(window, || unsafe { wi_companion_panel_drag() })? {
        Ok(())
    } else {
        Err("Could not drag companion panel".into())
    }
}
