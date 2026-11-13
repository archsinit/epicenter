//! Native-owned tray: available before any webview loads, with no resource-file lookup.
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};

const TRAY_ID: &str = "whispering-tray";

// Monochrome alpha mask; macOS templates adapt to light and dark menu bars.
fn microphone_icon(recording: bool) -> Image<'static> {
    let mut rgba = vec![0_u8; 18 * 18 * 4];
    for y in 0..18 {
        for x in 0..18 {
            let capsule = (6..=11).contains(&x)
                && (2..=10).contains(&y)
                && !((y == 2 || y == 10) && (x == 6 || x == 11));
            let cradle = ((x == 4 || x == 13) && (7..=11).contains(&y))
                || ((y == 12) && (5..=12).contains(&x));
            let stand = ((x == 8 || x == 9) && (13..=14).contains(&y))
                || (y == 15 && (6..=11).contains(&x));
            let dot = recording && (14..=16).contains(&x) && (1..=3).contains(&y);
            if capsule || cradle || stand || dot {
                rgba[(y * 18 + x) * 4 + 3] = 255;
            }
        }
    }
    Image::new_owned(rgba, 18, 18)
}

fn show_main(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.show()?;
        window.unminimize()?;
        window.set_focus()?;
    }
    Ok(())
}

pub fn create(app: &mut tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "tray-show", "Show Window", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "tray-settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(
        app,
        "tray-quit",
        "Quit Whispering Intel",
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&show, &settings, &quit])?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(microphone_icon(false))
        .icon_as_template(true)
        .tooltip("Whispering Intel")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let result = match event.id.as_ref() {
                "tray-show" => show_main(app),
                "tray-settings" => show_main(app).and_then(|_| {
                    app.emit_to(
                        "main",
                        "navigate-main-window",
                        serde_json::json!({ "path": "/settings" }),
                    )
                }),
                "tray-quit" => {
                    app.exit(0);
                    Ok(())
                }
                _ => Ok(()),
            };
            if let Err(error) = result {
                log::warn!("Menu-bar action failed: {}", error);
            }
        })
        .build(&*app)?;

    // Never remove the Dock entry before the replacement access point exists.
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    Ok(())
}

#[tauri::command]
pub fn set_tray_recording(recording: bool, app: tauri::AppHandle) -> Result<(), String> {
    let tray = app.tray_by_id(TRAY_ID).ok_or("Menu-bar icon unavailable")?;
    tray.set_icon(Some(microphone_icon(recording)))
        .map_err(|error| error.to_string())?;
    // tray-icon's macOS set_icon creates a fresh NSImage with template=false.
    // Reapply after EVERY replacement so AppKit tints it for the actual menu bar.
    #[cfg(target_os = "macos")]
    tray.set_icon_as_template(true)
        .map_err(|error| error.to_string())?;
    tray.set_tooltip(Some(if recording {
        "Whispering Intel — Recording"
    } else {
        "Whispering Intel"
    }))
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_icon_is_visible_with_transparent_background() {
        let icon = microphone_icon(false);
        assert_eq!((icon.width(), icon.height()), (18, 18));
        assert_eq!(icon.rgba()[3], 0);
        assert!(icon.rgba().chunks_exact(4).any(|pixel| pixel[3] == 255));
        assert!(icon
            .rgba()
            .chunks_exact(4)
            .all(|pixel| pixel[..3] == [0, 0, 0]));
    }

    #[test]
    fn recording_adds_a_status_mark() {
        let idle = microphone_icon(false);
        let recording = microphone_icon(true);
        let opaque = |image: &Image<'_>| image.rgba().chunks_exact(4).filter(|p| p[3] > 0).count();
        assert_eq!(opaque(&recording), opaque(&idle) + 9);
    }
}
