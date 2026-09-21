use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

#[derive(Default)]
pub struct VoiceRuntime {
    active: AtomicBool,
}

pub fn is_active(app: &tauri::AppHandle) -> bool {
    app.state::<VoiceRuntime>().active.load(Ordering::Acquire)
}

pub fn create(app: &tauri::App) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, "voice", WebviewUrl::App("index.html#voice".into()))
        .title("ACE 음성 명령")
        .inner_size(380.0, 350.0)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .center()
        .build()?;
    Ok(())
}

fn position_at_bottom_right(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()? else {
        return Ok(());
    };
    let window_size = window.outer_size()?;
    let monitor_size = monitor.size();
    let monitor_position = monitor.position();
    let x = monitor_position.x + monitor_size.width.saturating_sub(window_size.width + 32) as i32;
    let y = monitor_position.y + monitor_size.height.saturating_sub(window_size.height + 32) as i32;
    window.set_position(PhysicalPosition::new(x, y))
}

pub fn activate(app: &tauri::AppHandle) -> Result<(), String> {
    let voice = app
        .get_webview_window("voice")
        .ok_or_else(|| "Voice window is unavailable".to_owned())?;
    let first_activation = !app
        .state::<VoiceRuntime>()
        .active
        .swap(true, Ordering::AcqRel);
    let result = (|| {
        if first_activation {
            app.emit_to("voice", "ace-voice-activate", ())
                .map_err(|error| error.to_string())?;
        }
        position_at_bottom_right(&voice).map_err(|error| error.to_string())?;
        voice
            .set_always_on_top(true)
            .map_err(|error| error.to_string())?;
        voice.show().map_err(|error| error.to_string())?;
        voice.set_focus().map_err(|error| error.to_string())
    })();
    if result.is_err() {
        app.state::<VoiceRuntime>()
            .active
            .store(false, Ordering::Release);
        let _ = app.emit_to("voice", "ace-voice-reset", ());
    }
    result
}

pub fn shutdown(app: &tauri::AppHandle) -> Result<(), String> {
    if let Err(error) = crate::voice_recording::cancel_active(app) {
        eprintln!("ACE temporary voice recording cleanup failed: {error}");
    }
    app.state::<VoiceRuntime>()
        .active
        .store(false, Ordering::Release);
    app.emit_to("voice", "ace-voice-reset", ())
        .map_err(|error| error.to_string())?;
    if let Some(voice) = app.get_webview_window("voice") {
        voice.hide().map_err(|error| error.to_string())?;
    }
    Ok(())
}
#[tauri::command]
pub fn activate_voice_orb(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    activate(&app)
}
#[tauri::command]
pub fn hide_voice_overlay(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if !["main", "voice"].contains(&window.label()) {
        return Err("BLOCKED".into());
    }
    shutdown(&app)
}
#[tauri::command]
pub fn submit_voice_command(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    text: String,
) -> Result<(), String> {
    if window.label() != "voice" || text.trim().is_empty() || text.chars().count() > 200 {
        return Err("BLOCKED".into());
    }
    // Transient delivery to the authenticated main window; never written to disk or stdout.
    app.emit_to("main", "ace-voice-command", text.trim())
        .map_err(|_| "Voice command delivery failed")?;
    app.state::<VoiceRuntime>()
        .active
        .store(false, Ordering::Release);
    window
        .hide()
        .map_err(|_| "Voice window could not be hidden".into())
}
