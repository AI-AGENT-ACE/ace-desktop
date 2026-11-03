// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use tauri::Manager;
mod activity;
mod app_updates;
mod app_windows;
mod audio_input;
mod capture_files;
mod default_model;
mod file_operations;
mod local_commands;
mod local_stt;
mod native_approvals;
mod native_tools;
mod native_windows;
mod recycle_bin;
mod screen_capture;
#[cfg(windows)]
mod taskbar;
mod tray;
mod voice_overlay;
mod voice_recording;
#[cfg(all(windows, debug_assertions))]
mod wake_diagnostic_data;
#[cfg(all(windows, debug_assertions))]
mod wake_diagnostics;
#[cfg(windows)]
mod wake_kws;
#[cfg(windows)]
mod wake_setup_capture;
mod wake_word;
mod wake_word_setup;
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
    let app = tauri::Builder::default()
        .manage(tray::TrayState::default())
        .manage(app_updates::Updates::default())
        .manage(audio_input::AudioInputState::default())
        .manage(app_windows::WindowTargets::default())
        .manage(file_operations::FileApprovals::default())
        .manage(screen_capture::CaptureState::default())
        .manage(capture_files::CaptureFiles::default())
        .manage(voice_overlay::VoiceRuntime::default())
        .manage(voice_recording::VoiceRecordingState::default())
        .manage(wake_word::WakeWordRuntime::default())
        .manage(wake_word_setup::WakeWordSetupRuntime::default())
        .manage(native_tools::NativeToolState::default())
        .manage(native_approvals::Approvals::default())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(not(debug_assertions))]
            app_updates::register(app.handle())?;
            capture_files::start(app.handle());
            app.manage(local_commands::initialize(app.handle()));
            if let Err(error) = voice_recording::cleanup_expired(app.handle()) {
                eprintln!("ACE voice temp startup cleanup failed: {error}");
            }
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
            if let Err(error) = wake_word::restore_persisted(app.handle()) {
                eprintln!("ACE Wake Word restore failed: {error}");
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "capture-selector" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    screen_capture::cancel_active(window.app_handle());
                }
            }
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
            audio_input::get_audio_input_status,
            audio_input::set_audio_input,
            audio_input::start_audio_input,
            audio_input::stop_audio_input,
            hide_ace,
            local_commands::execute_local_command,
            local_commands::list_installed_apps,
            native_tools::execute_native_tool,
            native_tools::authorize_native_tool,
            native_tools::prepare_native_tool,
            native_tools::describe_native_target,
            native_tools::await_voice_activation,
            voice_overlay::report_voice_phase,
            app_windows::observe_window_close,
            file_operations::discard_file_approval,
            file_operations::authorize_file_overwrite,
            screen_capture::get_capture_context,
            screen_capture::complete_screen_capture,
            screen_capture::cancel_screen_capture,
            screen_capture::await_screen_capture,
            app_updates::update_status,
            app_updates::check_app_update,
            app_updates::install_app_update,
            capture_files::release_capture_file,
            capture_files::save_capture_file,
            voice_overlay::activate_voice_orb,
            voice_overlay::hide_voice_overlay,
            voice_overlay::submit_voice_command,
            voice_overlay::submit_voice_tool_call,
            voice_overlay::drag_voice_orb,
            voice_recording::start_voice_recording,
            voice_recording::append_voice_recording_samples,
            voice_recording::stop_voice_recording,
            voice_recording::cancel_voice_recording,
            voice_recording::discard_voice_recording,
            voice_recording::upload_voice_recording,
            voice_recording::transcribe_local_portfolio,
            voice_overlay::report_voice_tool_result,
            voice_overlay::request_tool_confirmation,
            voice_overlay::respond_voice_choice,
            voice_overlay::cancel_voice_request,
            tray::set_wake_word_enabled,
            wake_word::diagnose_audio_input,
            wake_word::get_wake_word_status,
            #[cfg(all(windows, debug_assertions))]
            wake_diagnostics::start_wake_diagnostic,
            #[cfg(all(windows, debug_assertions))]
            wake_diagnostics::stop_wake_diagnostic,
            #[cfg(all(windows, debug_assertions))]
            wake_diagnostics::get_wake_diagnostic,
            #[cfg(all(windows, debug_assertions))]
            wake_diagnostics::retain_wake_setup_diagnostics,
            wake_word_setup::get_wake_word_setup_status,
            wake_word_setup::set_wake_word_model,
            wake_word_setup::set_wake_word_sensitivity,
            wake_word_setup::prepare_wake_word_setup,
            wake_word_setup::start_wake_word_sample,
            wake_word_setup::start_wake_word_test_sample,
            wake_word_setup::finish_wake_word_sample,
            wake_word_setup::generate_wake_word_reference,
            wake_word_setup::test_wake_word_reference,
            wake_word_setup::complete_wake_word_setup,
            wake_word_setup::postpone_wake_word_setup,
            wake_word_setup::cancel_wake_word_setup,
            tray::take_pending_settings_request
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application");
    app.run(|app, event| {
        if matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) {
            wake_word::stop(app);
            screen_capture::cancel_active(app);
            audio_input::stop(app);
            if let Err(error) = voice_recording::cancel_active(app) {
                eprintln!("ACE recording exit cleanup failed: {error}");
            }
        }
    });
}
