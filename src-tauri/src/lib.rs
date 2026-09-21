// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use tauri::Manager;
mod local_commands;
#[cfg(windows)]
mod taskbar;
mod tray;
mod voice_overlay;
mod voice_recording;
#[tauri::command]
fn hide_ace(window: tauri::WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Only the ACE main window can be controlled".into());
    }
    if let Some(voice) = window.app_handle().get_webview_window("voice") {
        let _ = voice.hide();
    }
    window.hide().map_err(|error| error.to_string())
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(tray::TrayState::default())
        .manage(voice_overlay::VoiceRuntime::default())
        .manage(voice_recording::VoiceRecordingState::default())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(local_commands::initialize(app.handle()));
            voice_overlay::create(app)?;
            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                if let Err(error) = taskbar::configure(app.handle(), &window) {
                    eprintln!("ACE taskbar icon configuration failed: {error}");
                }
            }
            if let Some(icon) = app.default_window_icon() {
                // Use the enlarged transparent ACE icon for both the window and tray.
                if let Some(window) = app.get_webview_window("main") {
                    window.set_icon(icon.clone())?;
                }
            }
            tray::create(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Err(error) = window.hide() {
                        eprintln!("ACE could not be hidden: {error}");
                    }
                }
            }
            if window.label() == "voice" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Err(error) = voice_overlay::shutdown(window.app_handle()) {
                        eprintln!("ACE voice close cleanup failed: {error}");
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            hide_ace,
            local_commands::execute_local_command,
            local_commands::list_installed_apps,
            voice_overlay::activate_voice_orb,
            voice_overlay::hide_voice_overlay,
            voice_overlay::submit_voice_command,
            voice_recording::start_voice_recording,
            voice_recording::append_voice_recording_samples,
            voice_recording::stop_voice_recording,
            voice_recording::cancel_voice_recording,
            tray::set_wake_word_enabled,
            tray::take_pending_settings_request
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
