use crate::{
    local_commands::{resolve_app, NativeExecutionState},
    native_windows, voice_overlay,
};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

const LOG_FILE: &str = "native-tools.jsonl";
const MAX_CLIPBOARD: usize = 65_536;

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Risk {
    Safe,
    Confirm,
    Blocked,
}

#[derive(Serialize)]
pub struct NativeToolError {
    code: String,
    message: String,
}

#[derive(Serialize)]
pub struct NativeToolResult {
    success: bool,
    tool: String,
    data: Option<Value>,
    error: Option<NativeToolError>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditLog<'a> {
    tool: &'a str,
    risk: Risk,
    status: &'a str,
    duration_ms: u128,
    error_code: Option<&'a str>,
    timestamp_ms: u128,
}

fn policy(tool: &str) -> Risk {
    match tool {
        "system.status"
        | "app.open"
        | "app.focus"
        | "app.minimize"
        | "app.maximize"
        | "file.search"
        | "file.open"
        | "folder.open"
        | "folder.search"
        | "web.open"
        | "web.search"
        | "clipboard.write"
        | "voice.activate"
        | "screen.capture"
        | "system.volume.set"
        | "system.volume.mute"
        | "system.open_settings" => Risk::Safe,
        "app.close" | "file.rename" | "file.move" | "file.copy" | "file.delete"
        | "folder.create" | "clipboard.read" | "system.lock" => Risk::Confirm,
        _ => Risk::Blocked,
    }
}

fn ok(tool: &str, data: Value) -> NativeToolResult {
    NativeToolResult {
        success: true,
        tool: tool.into(),
        data: Some(data),
        error: None,
    }
}
fn fail(tool: &str, code: &str) -> NativeToolResult {
    let message = match code {
        "APP_NOT_FOUND" => "설치된 앱을 찾지 못했습니다.",
        "WINDOW_NOT_FOUND" => "실행 중인 앱 창을 찾지 못했습니다.",
        "FILE_NOT_FOUND" => "파일을 찾지 못했습니다.",
        "DIRECTORY_NOT_FOUND" => "폴더를 찾지 못했습니다.",
        "PATH_NOT_ALLOWED" => "허용된 위치 밖의 경로입니다.",
        "DESTINATION_EXISTS" => "대상 위치에 같은 이름이 이미 있습니다.",
        "CONFIRMATION_REQUIRED" => "사용자 확인이 필요합니다.",
        "BLOCKED_BY_POLICY" => "보안 정책으로 차단된 기능입니다.",
        "CLIPBOARD_DENIED" => "클립보드에 접근할 수 없습니다.",
        "SCREEN_CAPTURE_FAILED" => "화면을 캡처하지 못했습니다.",
        "PERMISSION_DENIED" => "운영체제 권한이 거부되었습니다.",
        _ => "인자가 올바르지 않거나 실행에 실패했습니다.",
    };
    NativeToolResult {
        success: false,
        tool: tool.into(),
        data: None,
        error: Some(NativeToolError {
            code: code.into(),
            message: message.into(),
        }),
    }
}

fn args<'a>(value: &'a Value, allowed: &[&str]) -> Result<&'a Map<String, Value>, &'static str> {
    let object = value.as_object().ok_or("INVALID_ARGUMENT")?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("INVALID_ARGUMENT");
    }
    Ok(object)
}
fn string<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, &'static str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty() && v.len() <= 2048)
        .ok_or("INVALID_ARGUMENT")
}
fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn root(name: &str) -> Result<PathBuf, &'static str> {
    let path = match name.to_ascii_lowercase().as_str() {
        "home" => dirs::home_dir(),
        "desktop" => dirs::desktop_dir(),
        "downloads" => dirs::download_dir(),
        "documents" => dirs::document_dir(),
        "pictures" => dirs::picture_dir(),
        "music" => dirs::audio_dir(),
        "videos" => dirs::video_dir(),
        _ => return Err("PATH_NOT_ALLOWED"),
    }
    .ok_or("DIRECTORY_NOT_FOUND")?;
    if path.is_dir() {
        Ok(path)
    } else {
        Err("DIRECTORY_NOT_FOUND")
    }
}
fn relative(value: &str) -> Result<PathBuf, &'static str> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err("PATH_NOT_ALLOWED");
    }
    Ok(path.to_path_buf())
}
fn single_name(value: &str) -> Result<&str, &'static str> {
    let value = value.trim();
    let mut components = Path::new(value).components();
    if value.is_empty()
        || value.len() > 255
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err("INVALID_ARGUMENT");
    }
    Ok(value)
}
fn existing(
    directory: &str,
    value: &str,
    directory_expected: bool,
) -> Result<PathBuf, &'static str> {
    let base = root(directory)?
        .canonicalize()
        .map_err(|_| "DIRECTORY_NOT_FOUND")?;
    let path = base.join(relative(value)?).canonicalize().map_err(|_| {
        if directory_expected {
            "DIRECTORY_NOT_FOUND"
        } else {
            "FILE_NOT_FOUND"
        }
    })?;
    if !path.starts_with(&base) {
        return Err("PATH_NOT_ALLOWED");
    }
    if directory_expected && !path.is_dir() {
        return Err("DIRECTORY_NOT_FOUND");
    }
    if !directory_expected && !path.is_file() {
        return Err("FILE_NOT_FOUND");
    }
    Ok(path)
}
fn destination(directory: &str, value: &str) -> Result<PathBuf, &'static str> {
    let base = root(directory)?
        .canonicalize()
        .map_err(|_| "DIRECTORY_NOT_FOUND")?;
    let relative = relative(value)?;
    let parent = base
        .join(&relative)
        .parent()
        .ok_or("PATH_NOT_ALLOWED")?
        .canonicalize()
        .map_err(|_| "DIRECTORY_NOT_FOUND")?;
    if !parent.starts_with(&base) {
        return Err("PATH_NOT_ALLOWED");
    }
    let path = base.join(relative);
    if path.exists() {
        Err("DESTINATION_EXISTS")
    } else {
        Ok(path)
    }
}

fn search(
    directory: &str,
    query: &str,
    extensions: &[String],
    folders: bool,
    limit: usize,
) -> Result<Vec<Value>, &'static str> {
    let base = root(directory)?
        .canonicalize()
        .map_err(|_| "DIRECTORY_NOT_FOUND")?;
    let query = query.to_lowercase();
    let mut stack = vec![(base.clone(), 0u8)];
    let mut found = Vec::new();
    let mut scanned = 0usize;
    while let Some((current, depth)) = stack.pop() {
        for entry in fs::read_dir(current)
            .map_err(|_| "PERMISSION_DENIED")?
            .flatten()
        {
            scanned += 1;
            if scanned > 5000 {
                break;
            }
            let path = entry.path();
            let is_dir = path.is_dir();
            if is_dir && depth < 4 {
                stack.push((path.clone(), depth + 1));
            }
            if is_dir != folders {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.to_lowercase().contains(&query) {
                continue;
            }
            if !folders && !extensions.is_empty() {
                let ext = path
                    .extension()
                    .and_then(|v| v.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !extensions.iter().any(|v| v == &ext) {
                    continue;
                }
            }
            found.push(json!({"name": name, "path": path.strip_prefix(&base).unwrap_or(&path).to_string_lossy()}));
            if found.len() >= limit {
                return Ok(found);
            }
        }
        if scanned > 5000 {
            break;
        }
    }
    Ok(found)
}

fn log(
    app: &tauri::AppHandle,
    tool: &str,
    risk: Risk,
    started: Instant,
    result: &NativeToolResult,
) {
    let Ok(dir) = app.path().app_log_dir() else {
        return;
    };
    let _ = fs::create_dir_all(&dir);
    let entry = AuditLog {
        tool,
        risk,
        status: if result.success { "SUCCESS" } else { "FAILED" },
        duration_ms: started.elapsed().as_millis(),
        error_code: result.error.as_ref().map(|e| e.code.as_str()),
        timestamp_ms: timestamp(),
    };
    if let Ok(line) = serde_json::to_string(&entry) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(LOG_FILE))
        {
            let _ = writeln!(file, "{line}");
        }
    }
}

#[tauri::command]
pub fn execute_native_tool(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, NativeExecutionState>,
    tool: String,
    arguments: Value,
    confirmed: bool,
) -> NativeToolResult {
    let started = Instant::now();
    let risk = policy(&tool);
    let result = if window.label() != "main" || risk == Risk::Blocked {
        fail(&tool, "BLOCKED_BY_POLICY")
    } else if risk == Risk::Confirm && !confirmed {
        fail(&tool, "CONFIRMATION_REQUIRED")
    } else {
        dispatch(&app, &state, &tool, &arguments).unwrap_or_else(|code| fail(&tool, code))
    };
    log(&app, &tool, risk, started, &result);
    result
}

fn dispatch(
    app: &tauri::AppHandle,
    state: &NativeExecutionState,
    tool: &str,
    arguments: &Value,
) -> Result<NativeToolResult, &'static str> {
    match tool {
        "system.status" => {
            args(arguments, &[])?;
            Ok(ok(
                tool,
                json!({"os": std::env::consts::OS, "arch": std::env::consts::ARCH, "cpuThreads": std::thread::available_parallelism().map(|v|v.get()).unwrap_or(1)}),
            ))
        }
        "app.open" | "app.close" => {
            let object = args(arguments, &["appId", "appName"])?;
            let id = object
                .get("appId")
                .or_else(|| object.get("appName"))
                .and_then(Value::as_str)
                .ok_or("INVALID_ARGUMENT")?;
            let registry = state.registry.lock().map_err(|_| "EXECUTION_FAILED")?;
            let target = resolve_app(&registry, id)?;
            let result = crate::local_commands::execute_app(tool, &target);
            let value = serde_json::to_value(result).map_err(|_| "EXECUTION_FAILED")?;
            if value.get("success").and_then(Value::as_bool) == Some(true) {
                Ok(ok(tool, json!({"appId":id})))
            } else {
                Err("APP_LAUNCH_FAILED")
            }
        }
        "app.focus" | "app.minimize" | "app.maximize" => {
            let object = args(arguments, &["appId", "appName"])?;
            let id = object
                .get("appId")
                .or_else(|| object.get("appName"))
                .and_then(Value::as_str)
                .ok_or("INVALID_ARGUMENT")?;
            let registry = state.registry.lock().map_err(|_| "EXECUTION_FAILED")?;
            let target = resolve_app(&registry, id)?;
            native_windows::control_window(&target.process_name, tool)?;
            Ok(ok(tool, json!({"appId":id})))
        }
        "file.search" | "folder.search" => {
            let o = args(arguments, &["directory", "query", "extensions", "limit"])?;
            let exts: Vec<String> = o
                .get("extensions")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(|s| s.trim_start_matches('.').to_ascii_lowercase())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let limit = o
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(20)
                .clamp(1, 50) as usize;
            let values = search(
                string(o, "directory")?,
                string(o, "query")?,
                &exts,
                tool == "folder.search",
                limit,
            )?;
            Ok(ok(tool, json!({"results":values})))
        }
        "file.open" => {
            let o = args(arguments, &["directory", "path"])?;
            let path = existing(string(o, "directory")?, string(o, "path")?, false)?;
            native_windows::open_with_shell(&path)?;
            Ok(ok(tool, json!({"opened":true})))
        }
        "folder.open" => {
            let o = args(arguments, &["directory", "path"])?;
            let path = if let Some(v) = o.get("path").and_then(Value::as_str) {
                existing(string(o, "directory")?, v, true)?
            } else {
                root(string(o, "directory")?)?
            };
            native_windows::open_with_shell(&path)?;
            Ok(ok(tool, json!({"opened":true})))
        }
        "folder.create" => {
            let o = args(arguments, &["parent", "name"])?;
            let path = destination(string(o, "parent")?, single_name(string(o, "name")?)?)?;
            fs::create_dir(&path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    "PERMISSION_DENIED"
                } else {
                    "INVALID_ARGUMENT"
                }
            })?;
            Ok(ok(tool, json!({"created":true})))
        }
        "file.rename" => {
            let o = args(arguments, &["directory", "path", "newName"])?;
            let source = existing(string(o, "directory")?, string(o, "path")?, false)?;
            let new_name = single_name(string(o, "newName")?)?;
            let target = source.parent().ok_or("PATH_NOT_ALLOWED")?.join(new_name);
            if target.exists() {
                return Err("DESTINATION_EXISTS");
            }
            fs::rename(source, target).map_err(|_| "PERMISSION_DENIED")?;
            Ok(ok(tool, json!({"renamed":true})))
        }
        "file.move" | "file.copy" => {
            let o = args(
                arguments,
                &[
                    "sourceDirectory",
                    "sourcePath",
                    "destinationDirectory",
                    "destinationPath",
                ],
            )?;
            let source = existing(
                string(o, "sourceDirectory")?,
                string(o, "sourcePath")?,
                false,
            )?;
            let target = destination(
                string(o, "destinationDirectory")?,
                string(o, "destinationPath")?,
            )?;
            if tool == "file.copy" {
                fs::copy(source, target).map_err(|_| "PERMISSION_DENIED")?;
            } else {
                fs::rename(source, target).map_err(|_| "PERMISSION_DENIED")?;
            }
            Ok(ok(tool, json!({"completed":true})))
        }
        "file.delete" => {
            let o = args(arguments, &["directory", "path"])?;
            let path = existing(string(o, "directory")?, string(o, "path")?, false)?;
            native_windows::recycle(&path)?;
            Ok(ok(tool, json!({"recycled":true})))
        }
        "web.open" => {
            let o = args(arguments, &["url"])?;
            let url = url::Url::parse(string(o, "url")?).map_err(|_| "INVALID_ARGUMENT")?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err("PATH_NOT_ALLOWED");
            }
            native_windows::open_with_shell(Path::new(url.as_str()))?;
            Ok(ok(tool, json!({"opened":true})))
        }
        "web.search" => {
            let o = args(arguments, &["engine", "query"])?;
            let base = match string(o, "engine")?.to_ascii_lowercase().as_str() {
                "google" => "https://www.google.com/search",
                "bing" => "https://www.bing.com/search",
                "duckduckgo" => "https://duckduckgo.com/",
                _ => return Err("INVALID_ARGUMENT"),
            };
            let mut url = url::Url::parse(base).unwrap();
            url.query_pairs_mut().append_pair("q", string(o, "query")?);
            native_windows::open_with_shell(Path::new(url.as_str()))?;
            Ok(ok(tool, json!({"opened":true})))
        }
        "clipboard.write" => {
            let o = args(arguments, &["text"])?;
            let text = string(o, "text")?;
            if text.len() > MAX_CLIPBOARD {
                return Err("INVALID_ARGUMENT");
            }
            native_windows::clipboard_write(text)?;
            Ok(ok(tool, json!({"written":true})))
        }
        "clipboard.read" => {
            args(arguments, &[])?;
            let text = native_windows::clipboard_read()?;
            Ok(ok(tool, json!({"text":text})))
        }
        "voice.activate" => {
            args(arguments, &[])?;
            voice_overlay::activate(app).map_err(|_| "EXECUTION_FAILED")?;
            Ok(ok(tool, json!({"state":"LISTENING"})))
        }
        "screen.capture" => {
            args(arguments, &[])?;
            let dir = app
                .path()
                .app_data_dir()
                .map_err(|_| "SCREEN_CAPTURE_FAILED")?
                .join("screenshots");
            fs::create_dir_all(&dir).map_err(|_| "SCREEN_CAPTURE_FAILED")?;
            let path = dir.join(format!("ace-capture-{}.png", timestamp()));
            native_windows::capture_primary(&path)?;
            Ok(ok(tool, json!({"path":path.to_string_lossy()})))
        }
        "system.open_settings" => {
            let o = args(arguments, &["page"])?;
            let page = o.get("page").and_then(Value::as_str).unwrap_or("");
            if page.len() > 80
                || !page
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'))
            {
                return Err("INVALID_ARGUMENT");
            }
            let uri = if page.is_empty() {
                "ms-settings:".into()
            } else {
                format!("ms-settings:{page}")
            };
            native_windows::open_with_shell(Path::new(&uri))?;
            Ok(ok(tool, json!({"opened":true})))
        }
        "system.lock" => {
            args(arguments, &[])?;
            native_windows::lock_workstation()?;
            Ok(ok(tool, json!({"locked":true})))
        }
        "system.volume.set" => {
            let o = args(arguments, &["level"])?;
            let level = o
                .get("level")
                .and_then(Value::as_f64)
                .filter(|v| (0.0..=100.0).contains(v))
                .ok_or("INVALID_ARGUMENT")?;
            native_windows::set_volume(Some(level as f32 / 100.0), None)?;
            Ok(ok(tool, json!({"level":level})))
        }
        "system.volume.mute" => {
            let o = args(arguments, &["mute"])?;
            let mute = o
                .get("mute")
                .and_then(Value::as_bool)
                .ok_or("INVALID_ARGUMENT")?;
            native_windows::set_volume(None, Some(mute))?;
            Ok(ok(tool, json!({"mute":mute})))
        }
        _ => Err("BLOCKED_BY_POLICY"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn dangerous_tools_are_blocked() {
        for tool in [
            "shell.exec",
            "system.shutdown",
            "system.restart",
            "registry.write",
            "service.modify",
            "firewall.modify",
        ] {
            assert!(policy(tool) == Risk::Blocked)
        }
    }
    #[test]
    fn native_risks_are_owned_by_rust() {
        assert!(policy("app.open") == Risk::Safe);
        assert!(policy("file.search") == Risk::Safe);
        assert!(policy("file.delete") == Risk::Confirm);
        assert!(policy("clipboard.read") == Risk::Confirm);
    }
    #[test]
    fn traversal_and_absolute_paths_are_rejected() {
        assert_eq!(relative("../secret").unwrap_err(), "PATH_NOT_ALLOWED");
        assert_eq!(relative("C:\\secret").unwrap_err(), "PATH_NOT_ALLOWED")
    }
    #[test]
    fn unexpected_arguments_are_rejected() {
        assert_eq!(
            args(
                &json!({"url":"https://example.com","command":"x"}),
                &["url"]
            )
            .unwrap_err(),
            "INVALID_ARGUMENT"
        )
    }
    #[test]
    fn folder_names_cannot_smuggle_nested_paths() {
        assert_eq!(single_name("ACE").unwrap(), "ACE");
        assert_eq!(single_name("nested/ACE").unwrap_err(), "INVALID_ARGUMENT");
    }
}
