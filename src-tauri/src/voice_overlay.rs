use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub fn create(app: &tauri::App) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, "voice", WebviewUrl::App("index.html#voice".into()))
        .title("ACE 음성 명령")
        .inner_size(380.0, 280.0)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .center()
        .build()?;
    Ok(())
}
#[tauri::command]
pub fn show_voice_overlay(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    let voice = app
        .get_webview_window("voice")
        .ok_or("Voice window unavailable")?;
    let _ = app.emit_to("voice", "ace-voice-reset", ());
    voice
        .set_always_on_top(true)
        .and_then(|_| voice.show())
        .and_then(|_| voice.set_focus())
        .map_err(|_| "Voice window could not be opened".into())
}
#[tauri::command]
pub fn hide_voice_overlay(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if !["main", "voice"].contains(&window.label()) {
        return Err("BLOCKED".into());
    }
    let _ = app.emit_to("voice", "ace-voice-reset", ());
    app.get_webview_window("voice")
        .ok_or("Voice window unavailable")?
        .hide()
        .map_err(|_| "Voice window could not be hidden".into())
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
    window
        .hide()
        .map_err(|_| "Voice window could not be hidden".into())
}
