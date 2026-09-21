use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

use crate::voice_overlay;

const OPEN_MAIN: &str = "open_main";
const VOICE_COMMAND: &str = "voice_command";
const HIDE_MAIN: &str = "hide_main";
const TOGGLE_WAKE_WORD: &str = "toggle_wake_word";
const OPEN_SETTINGS: &str = "open_settings";
const QUIT: &str = "quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayAction {
    OpenMain,
    VoiceCommand,
    HideMain,
    ToggleWakeWord,
    OpenSettings,
    Quit,
}

impl TrayAction {
    fn from_id(id: &str) -> Option<Self> {
        match id {
            OPEN_MAIN => Some(Self::OpenMain),
            VOICE_COMMAND => Some(Self::VoiceCommand),
            HIDE_MAIN => Some(Self::HideMain),
            TOGGLE_WAKE_WORD => Some(Self::ToggleWakeWord),
            OPEN_SETTINGS => Some(Self::OpenSettings),
            QUIT => Some(Self::Quit),
            _ => None,
        }
    }
}

pub struct TrayState {
    wake_word_enabled: AtomicBool,
    open_settings_pending: AtomicBool,
    wake_word_item: Mutex<Option<MenuItem<tauri::Wry>>>,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            wake_word_enabled: AtomicBool::new(true),
            open_settings_pending: AtomicBool::new(false),
            wake_word_item: Mutex::new(None),
        }
    }
}

impl TrayState {
    fn wake_word_enabled(&self) -> bool {
        self.wake_word_enabled.load(Ordering::Acquire)
    }

    fn set_wake_word(&self, enabled: bool) -> Result<(), String> {
        self.wake_word_enabled.store(enabled, Ordering::Release);
        let item = self
            .wake_word_item
            .lock()
            .map_err(|_| "Tray wake word state is unavailable".to_owned())?;
        if let Some(item) = item.as_ref() {
            item.set_text(wake_word_text(enabled))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

fn wake_word_text(enabled: bool) -> &'static str {
    if enabled {
        "Wake Word: 켜짐"
    } else {
        "Wake Word: 꺼짐"
    }
}

fn report(action: TrayAction, error: impl std::fmt::Display) {
    eprintln!("ACE tray action {action:?} failed: {error}");
}

pub fn open_main(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window is unavailable".to_owned())?;
    window.show().map_err(|error| error.to_string())?;
    window.unminimize().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

fn hide_main(app: &AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "Main window is unavailable".to_owned())?
        .hide()
        .map_err(|error| error.to_string())
}

fn toggle_wake_word(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<TrayState>();
    let enabled = !state.wake_word_enabled();
    state.set_wake_word(enabled)?;
    app.emit_to("main", "ace-wake-word-changed", enabled)
        .map_err(|error| error.to_string())
}

fn open_settings(app: &AppHandle) -> Result<(), String> {
    open_main(app)?;
    app.state::<TrayState>()
        .open_settings_pending
        .store(true, Ordering::Release);
    app.emit_to("main", "ace-open-settings", ())
        .map_err(|error| error.to_string())
}

fn quit(app: &AppHandle) {
    if let Err(error) = voice_overlay::shutdown(app) {
        report(TrayAction::Quit, error);
    }
    app.exit(0);
}

fn handle(app: &AppHandle, action: TrayAction) {
    let result = match action {
        TrayAction::OpenMain => open_main(app),
        TrayAction::VoiceCommand => voice_overlay::activate(app),
        TrayAction::HideMain => hide_main(app),
        TrayAction::ToggleWakeWord => toggle_wake_word(app),
        TrayAction::OpenSettings => open_settings(app),
        TrayAction::Quit => {
            quit(app);
            return;
        }
    };
    if let Err(error) = result {
        report(action, error);
    }
}

pub fn create(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, OPEN_MAIN, "ACE 열기", true, None::<&str>)?;
    let voice = MenuItem::with_id(app, VOICE_COMMAND, "음성 호출", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, HIDE_MAIN, "ACE 숨기기", true, None::<&str>)?;
    let first_separator = PredefinedMenuItem::separator(app)?;
    let wake = MenuItem::with_id(
        app,
        TOGGLE_WAKE_WORD,
        wake_word_text(app.state::<TrayState>().wake_word_enabled()),
        true,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(app, OPEN_SETTINGS, "설정", true, None::<&str>)?;
    let second_separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, QUIT, "종료", true, None::<&str>)?;
    *app.state::<TrayState>()
        .wake_word_item
        .lock()
        .map_err(|_| tauri::Error::AssetNotFound("tray state".into()))? = Some(wake.clone());
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &voice,
            &hide,
            &first_separator,
            &wake,
            &settings,
            &second_separator,
            &quit,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id("ace-tray")
        .tooltip("ACE · Auto Computer Executor")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if let Some(action) = TrayAction::from_id(event.id.as_ref()) {
                handle(app, action);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                handle(tray.app_handle(), TrayAction::OpenMain);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[tauri::command]
pub fn set_wake_word_enabled(
    app: AppHandle,
    window: tauri::WebviewWindow,
    enabled: bool,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    app.state::<TrayState>().set_wake_word(enabled)
}

#[tauri::command]
pub fn take_pending_settings_request(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<bool, String> {
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    Ok(app
        .state::<TrayState>()
        .open_settings_pending
        .swap(false, Ordering::AcqRel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_map_to_central_actions() {
        assert_eq!(
            TrayAction::from_id(VOICE_COMMAND),
            Some(TrayAction::VoiceCommand)
        );
        assert_eq!(
            TrayAction::from_id(OPEN_SETTINGS),
            Some(TrayAction::OpenSettings)
        );
        assert_eq!(TrayAction::from_id("unknown"), None);
    }

    #[test]
    fn wake_word_labels_reflect_runtime_state() {
        assert_eq!(wake_word_text(true), "Wake Word: 켜짐");
        assert_eq!(wake_word_text(false), "Wake Word: 꺼짐");
    }
}
