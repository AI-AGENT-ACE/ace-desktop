// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use tauri::{Emitter, Manager};
mod local_commands;
#[cfg(windows)]
mod taskbar;
mod voice_overlay;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

fn open_ace(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window
            .show()
            .and_then(|_| window.unminimize())
            .and_then(|_| window.set_focus())
        {
            eprintln!("ACE could not be opened: {error}");
        }
    }
}
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
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            voice_overlay::create(app)?;
            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                if let Err(error) = taskbar::configure(app.handle(), &window) {
                    eprintln!("ACE taskbar icon configuration failed: {error}");
                }
            }
            let open = MenuItem::with_id(app, "open-ace", "ACE 열기", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit-ace", "ACE 종료", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let mut tray = TrayIconBuilder::with_id("ace-tray")
                .tooltip("ACE · 개인 AI 어시스턴트")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open-ace" => open_ace(app),
                    "quit-ace" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        open_ace(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                // Use the enlarged transparent ACE icon for both the window and tray.
                if let Some(window) = app.get_webview_window("main") {
                    window.set_icon(icon.clone())?;
                }
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Some(voice) = window.app_handle().get_webview_window("voice") {
                        let _ = voice.hide();
                        let _ = window.app_handle().emit_to("voice", "ace-voice-reset", ());
                    }
                    if let Err(error) = window.hide() {
                        eprintln!("ACE could not be hidden: {error}");
                    }
                }
            }
            if window.label() == "voice" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.app_handle().emit_to("voice", "ace-voice-reset", ());
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            hide_ace,
            local_commands::execute_local_command,
            voice_overlay::show_voice_overlay,
            voice_overlay::hide_voice_overlay,
            voice_overlay::submit_voice_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
