use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{fs, path::PathBuf};
use tauri::{Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

#[derive(Default)]
pub struct VoiceRuntime {
    active: AtomicBool,
}

const VOICE_WIDTH: f64 = 260.0;
const VOICE_HEIGHT: f64 = 220.0;
const EDGE_MARGIN: i32 = 28;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct SavedPosition {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

pub fn is_active(app: &tauri::AppHandle) -> bool {
    app.state::<VoiceRuntime>().active.load(Ordering::Acquire)
}

pub fn create(app: &tauri::App) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, "voice", WebviewUrl::App("index.html#voice".into()))
        .title("ACE 음성 명령")
        .inner_size(VOICE_WIDTH, VOICE_HEIGHT)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .center()
        .build()?;
    Ok(())
}

fn position_file(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|directory| directory.join("voice-orb-position.json"))
        .map_err(|error| error.to_string())
}

fn work_area(monitor: &tauri::Monitor) -> Bounds {
    let area = monitor.work_area();
    Bounds {
        x: area.position.x,
        y: area.position.y,
        width: area.size.width,
        height: area.size.height,
    }
}

fn clamp(
    position: SavedPosition,
    window_width: u32,
    window_height: u32,
    bounds: Bounds,
) -> SavedPosition {
    let maximum_x = bounds
        .x
        .saturating_add(bounds.width.saturating_sub(window_width) as i32);
    let maximum_y = bounds
        .y
        .saturating_add(bounds.height.saturating_sub(window_height) as i32);
    SavedPosition {
        x: position.x.clamp(bounds.x, maximum_x),
        y: position.y.clamp(bounds.y, maximum_y),
    }
}

fn inside(position: SavedPosition, window_width: u32, window_height: u32, bounds: Bounds) -> bool {
    position.x >= bounds.x
        && position.y >= bounds.y
        && position.x.saturating_add(window_width as i32)
            <= bounds.x.saturating_add(bounds.width as i32)
        && position.y.saturating_add(window_height as i32)
            <= bounds.y.saturating_add(bounds.height as i32)
}

fn load_position(app: &tauri::AppHandle) -> Option<SavedPosition> {
    let path = position_file(app).ok()?;
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}
fn save_position(app: &tauri::AppHandle, position: SavedPosition) -> Result<(), String> {
    let path = position_file(app)?;
    let parent = path.parent().ok_or("Voice position path unavailable")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec(&position).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(temporary, path).map_err(|e| e.to_string())
}

fn default_monitor(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
) -> Result<Option<tauri::Monitor>, String> {
    if let Some(main) = app.get_webview_window("main") {
        if let Ok(Some(monitor)) = main.current_monitor() {
            return Ok(Some(monitor));
        }
    }
    window.primary_monitor().map_err(|e| e.to_string())
}

fn restore_position(app: &tauri::AppHandle, window: &tauri::WebviewWindow) -> Result<(), String> {
    let window_size = window.outer_size().map_err(|error| error.to_string())?;
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let saved = load_position(app).filter(|position| {
        monitors.iter().any(|monitor| {
            inside(
                *position,
                window_size.width,
                window_size.height,
                work_area(monitor),
            )
        })
    });
    let position = if let Some(saved) = saved {
        saved
    } else {
        let Some(monitor) = default_monitor(app, window)? else {
            return Ok(());
        };
        let bounds = work_area(&monitor);
        SavedPosition {
            x: bounds.x + bounds.width.saturating_sub(window_size.width) as i32 - EDGE_MARGIN,
            y: bounds.y + bounds.height.saturating_sub(window_size.height) as i32 - EDGE_MARGIN,
        }
    };
    window
        .set_position(PhysicalPosition::new(position.x, position.y))
        .map_err(|e| e.to_string())?;
    let _ = save_position(app, position);
    Ok(())
}

fn clamp_and_save(app: &tauri::AppHandle, window: &tauri::WebviewWindow) -> Result<(), String> {
    let position = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let center_x = position.x.saturating_add(size.width as i32 / 2);
    let center_y = position.y.saturating_add(size.height as i32 / 2);
    let monitor = monitors
        .iter()
        .find(|monitor| {
            let b = work_area(monitor);
            center_x >= b.x
                && center_y >= b.y
                && center_x < b.x + b.width as i32
                && center_y < b.y + b.height as i32
        })
        .or_else(|| monitors.first())
        .ok_or("No monitor available")?;
    let clamped = clamp(
        SavedPosition {
            x: position.x,
            y: position.y,
        },
        size.width,
        size.height,
        work_area(monitor),
    );
    window
        .set_position(PhysicalPosition::new(clamped.x, clamped.y))
        .map_err(|e| e.to_string())?;
    save_position(app, clamped)
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
        restore_position(app, &voice)?;
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

#[tauri::command]
pub fn drag_voice_orb(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    if window.label() != "voice" {
        return Err("BLOCKED".into());
    }
    window.start_dragging().map_err(|e| e.to_string())?;
    if let Err(error) = clamp_and_save(&app, &window) {
        eprintln!("ACE voice position save failed: {error}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clamps_orb_inside_work_area() {
        let bounds = Bounds {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        };
        assert_eq!(
            (
                clamp(SavedPosition { x: -50, y: 1100 }, 260, 220, bounds).x,
                clamp(SavedPosition { x: -50, y: 1100 }, 260, 220, bounds).y
            ),
            (0, 820)
        );
    }
    #[test]
    fn validates_saved_position_after_monitor_change() {
        let bounds = Bounds {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        };
        assert!(inside(SavedPosition { x: 1600, y: 700 }, 260, 220, bounds));
        assert!(!inside(SavedPosition { x: 2200, y: 700 }, 260, 220, bounds));
    }
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

#[tauri::command]
pub fn submit_voice_tool_call(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    tool: String,
    arguments: Value,
) -> Result<(), String> {
    if window.label() != "voice" || tool.is_empty() || tool.len() > 100 || !arguments.is_object() {
        return Err("BLOCKED".into());
    }
    app.emit_to(
        "main",
        "ace-voice-tool-call",
        json!({ "tool": tool, "arguments": arguments }),
    )
    .map_err(|_| "Voice tool delivery failed")?;
    app.state::<VoiceRuntime>()
        .active
        .store(false, Ordering::Release);
    window
        .hide()
        .map_err(|_| "Voice window could not be hidden".into())
}
