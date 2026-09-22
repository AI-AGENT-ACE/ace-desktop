use crate::{
    local_commands::{InstalledApp, NativeExecutionState},
    native_windows, voice_overlay,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

const LOG_FILE: &str = "native-tools.jsonl";
const MAX_CLIPBOARD: usize = 65_536;
const MAX_CANDIDATES: usize = 5;
const MAX_CANDIDATE_LENGTH: usize = 100;
const RESOURCE_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EntityReference {
    #[serde(default)]
    canonical_id: Option<String>,
    original: String,
    #[serde(default)]
    candidates: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SearchQueryReference {
    original: String,
    #[serde(default)]
    candidates: Vec<String>,
}

#[derive(Clone)]
struct ResourceEntry {
    path: PathBuf,
    directory: bool,
    expires_at: Instant,
}

#[derive(Default)]
pub struct NativeToolState {
    resources: Mutex<HashMap<String, ResourceEntry>>,
}

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
        "AMBIGUOUS_APP_MATCH" => "여러 앱이 비슷하게 일치해 대상을 확정할 수 없습니다.",
        "AMBIGUOUS_FILE_MATCH" => "여러 대상이 일치해 하나를 확정할 수 없습니다.",
        "NO_MATCH_FOUND" => "일치하는 대상을 찾지 못했습니다.",
        "INVALID_CANDIDATES" => "대상 후보 형식이 올바르지 않습니다.",
        "RESOURCE_EXPIRED" => "검색 결과의 사용 시간이 만료되었습니다. 다시 검색해 주세요.",
        "RESOURCE_NOT_FOUND" => "검색 결과를 찾지 못했습니다. 다시 검색해 주세요.",
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

fn normalized(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn validated_terms(original: &str, candidates: &[String]) -> Result<Vec<String>, &'static str> {
    if original.trim().is_empty()
        || original.len() > MAX_CANDIDATE_LENGTH
        || candidates.len() > MAX_CANDIDATES
    {
        return Err("INVALID_CANDIDATES");
    }
    let mut seen = HashSet::new();
    let mut values = Vec::new();
    for value in std::iter::once(original).chain(candidates.iter().map(String::as_str)) {
        let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
        if value.is_empty() {
            continue;
        }
        if value.len() > MAX_CANDIDATE_LENGTH {
            return Err("INVALID_CANDIDATES");
        }
        let key = normalized(&value);
        if !key.is_empty() && seen.insert(key) {
            values.push(value);
        }
    }
    if values.is_empty() {
        Err("INVALID_CANDIDATES")
    } else {
        Ok(values)
    }
}

fn entity(value: &Value) -> Result<(Option<String>, Vec<String>), &'static str> {
    let reference: EntityReference =
        serde_json::from_value(value.clone()).map_err(|_| "INVALID_CANDIDATES")?;
    let canonical = reference
        .canonical_id
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty() && v.len() <= 80);
    let terms = validated_terms(&reference.original, &reference.candidates)?;
    Ok((canonical, terms))
}

fn query_reference(value: &Value) -> Result<Vec<String>, &'static str> {
    let reference: SearchQueryReference =
        serde_json::from_value(value.clone()).map_err(|_| "INVALID_CANDIDATES")?;
    validated_terms(&reference.original, &reference.candidates)
}

fn app_score(app: &InstalledApp, canonical: Option<&str>, terms: &[String]) -> u16 {
    let name = normalized(&app.name);
    let aliases: Vec<String> = app.aliases.iter().map(|value| normalized(value)).collect();
    let canonical_score = canonical
        .map(normalized)
        .map(|value| {
            if value == name || aliases.iter().any(|alias| alias == &value) {
                100
            } else {
                0
            }
        })
        .unwrap_or(0);
    terms
        .iter()
        .enumerate()
        .fold(canonical_score, |best, (index, term)| {
            let term = normalized(term);
            let score = if term == name {
                98
            } else if aliases.iter().any(|alias| alias == &term) {
                95
            } else if term.len() >= 3
                && (name.contains(&term) || aliases.iter().any(|alias| alias.contains(&term)))
            {
                75u16.saturating_sub(index as u16)
            } else {
                0
            };
            best.max(score)
        })
}

fn resolve_application(
    registry: &[InstalledApp],
    canonical: Option<&str>,
    terms: &[String],
) -> Result<InstalledApp, &'static str> {
    let mut scored: Vec<(u16, &InstalledApp)> = registry
        .iter()
        .filter_map(|app| {
            let score = app_score(app, canonical, terms);
            (score >= 70).then_some((score, app))
        })
        .collect();
    scored.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.name.cmp(&right.1.name))
    });
    let Some((score, winner)) = scored.first() else {
        return Err("APP_NOT_FOUND");
    };
    if scored.get(1).is_some_and(|second| second.0 == *score) {
        return Err("AMBIGUOUS_APP_MATCH");
    }
    #[allow(unused_mut)]
    let mut resolved = (*winner).clone();
    #[cfg(not(test))]
    {
        resolved.executable_path =
            crate::local_commands::verified_executable(&resolved.executable_path)
                .ok_or("APP_NOT_FOUND")?;
    }
    Ok(resolved)
}

fn resolve_folder(reference: &Value) -> Result<&'static str, &'static str> {
    let (canonical, terms) = entity(reference)?;
    let aliases: &[(&str, &[&str])] = &[
        ("desktop", &["desktop", "바탕화면"]),
        ("downloads", &["downloads", "download", "다운로드"]),
        ("documents", &["documents", "document", "문서"]),
        ("pictures", &["pictures", "picture", "사진", "이미지"]),
        ("music", &["music", "음악"]),
        ("videos", &["videos", "video", "동영상", "비디오"]),
        ("home", &["home", "홈", "사용자 폴더"]),
    ];
    let mut matches = Vec::new();
    for (id, names) in aliases {
        let exact_canonical = canonical
            .as_deref()
            .is_some_and(|value| normalized(value) == *id);
        let exact_term = terms.iter().any(|term| {
            names
                .iter()
                .any(|name| normalized(term) == normalized(name))
        });
        if exact_canonical || exact_term {
            matches.push(*id);
        }
    }
    matches.sort_unstable();
    matches.dedup();
    if matches.len() == 1 {
        Ok(matches[0])
    } else if matches.is_empty() {
        Err("NO_MATCH_FOUND")
    } else {
        Err("AMBIGUOUS_FILE_MATCH")
    }
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

struct SearchHit {
    path: PathBuf,
    name: String,
    score: u16,
}

fn search(
    directory: &str,
    queries: &[String],
    extensions: &[String],
    folders: bool,
    limit: usize,
) -> Result<Vec<SearchHit>, &'static str> {
    let base = root(directory)?
        .canonicalize()
        .map_err(|_| "DIRECTORY_NOT_FOUND")?;
    search_in(&base, queries, extensions, folders, limit)
}

fn search_in(
    base: &Path,
    queries: &[String],
    extensions: &[String],
    folders: bool,
    limit: usize,
) -> Result<Vec<SearchHit>, &'static str> {
    let base = base.canonicalize().map_err(|_| "DIRECTORY_NOT_FOUND")?;
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
            let name = entry.file_name().to_string_lossy().to_string();
            let normalized_name = normalized(&name);
            let stem = path
                .file_stem()
                .and_then(|v| v.to_str())
                .map(normalized)
                .unwrap_or_default();
            let score = queries
                .iter()
                .enumerate()
                .filter_map(|(index, query)| {
                    let query = normalized(query);
                    if query.is_empty() {
                        return None;
                    }
                    let base: u16 = if normalized_name == query {
                        100
                    } else if stem == query {
                        98
                    } else if normalized_name.starts_with(&query) {
                        90
                    } else if normalized_name.contains(&query) {
                        80
                    } else {
                        0
                    };
                    (base > 0).then_some(base.saturating_sub(index.min(10) as u16))
                })
                .max()
                .unwrap_or(0);
            if score > 0 {
                found.push(SearchHit { path, name, score });
            }
        }
        if scanned > 5000 {
            break;
        }
    }
    found.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.name.cmp(&right.name))
    });
    found.truncate(limit);
    Ok(found)
}

fn register_resource(
    state: &NativeToolState,
    path: PathBuf,
    directory: bool,
) -> Result<String, &'static str> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    timestamp().hash(&mut hasher);
    let id = format!(
        "{}_{}",
        if directory { "folder" } else { "file" },
        hasher.finish()
    );
    let mut resources = state.resources.lock().map_err(|_| "RESOURCE_NOT_FOUND")?;
    let now = Instant::now();
    resources.retain(|_, entry| entry.expires_at > now);
    resources.insert(
        id.clone(),
        ResourceEntry {
            path,
            directory,
            expires_at: now + RESOURCE_TTL,
        },
    );
    Ok(id)
}

fn resource(state: &NativeToolState, id: &str, directory: bool) -> Result<PathBuf, &'static str> {
    if id.len() > 100 {
        return Err("RESOURCE_NOT_FOUND");
    }
    let mut resources = state.resources.lock().map_err(|_| "RESOURCE_NOT_FOUND")?;
    let now = Instant::now();
    if resources
        .get(id)
        .is_some_and(|entry| entry.expires_at <= now)
    {
        resources.remove(id);
        return Err("RESOURCE_EXPIRED");
    }
    let entry = resources.get(id).ok_or("RESOURCE_NOT_FOUND")?;
    if entry.directory != directory || !entry.path.exists() {
        return Err("RESOURCE_NOT_FOUND");
    }
    Ok(entry.path.clone())
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
    tool_state: tauri::State<'_, NativeToolState>,
    version: Option<String>,
    tool: String,
    arguments: Value,
    confirmed: bool,
) -> NativeToolResult {
    let started = Instant::now();
    let risk = policy(&tool);
    let result = if version.as_deref().is_some_and(|value| value != "1.1") {
        fail(&tool, "INVALID_ARGUMENT")
    } else if window.label() != "main" || risk == Risk::Blocked {
        fail(&tool, "BLOCKED_BY_POLICY")
    } else if risk == Risk::Confirm && !confirmed {
        fail(&tool, "CONFIRMATION_REQUIRED")
    } else {
        dispatch(&app, &state, &tool_state, &tool, &arguments)
            .unwrap_or_else(|code| fail(&tool, code))
    };
    log(&app, &tool, risk, started, &result);
    result
}

fn dispatch(
    app: &tauri::AppHandle,
    state: &NativeExecutionState,
    tool_state: &NativeToolState,
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
            let object = args(
                arguments,
                &["canonicalId", "original", "candidates", "appId", "appName"],
            )?;
            let (canonical, terms) = if object.contains_key("original") {
                entity(arguments)?
            } else {
                let legacy = object
                    .get("appId")
                    .or_else(|| object.get("appName"))
                    .and_then(Value::as_str)
                    .ok_or("INVALID_CANDIDATES")?;
                (Some(legacy.to_owned()), vec![legacy.to_owned()])
            };
            let registry = state.registry.lock().map_err(|_| "EXECUTION_FAILED")?;
            let target = resolve_application(&registry, canonical.as_deref(), &terms)?;
            let result = crate::local_commands::execute_app(tool, &target);
            let value = serde_json::to_value(result).map_err(|_| "EXECUTION_FAILED")?;
            if value.get("success").and_then(Value::as_bool) == Some(true) {
                Ok(ok(
                    tool,
                    json!({"appId":normalized(&target.name),"displayName":target.name}),
                ))
            } else {
                Err("APP_LAUNCH_FAILED")
            }
        }
        "app.focus" | "app.minimize" | "app.maximize" => {
            let object = args(
                arguments,
                &["canonicalId", "original", "candidates", "appId", "appName"],
            )?;
            let (canonical, terms) = if object.contains_key("original") {
                entity(arguments)?
            } else {
                let legacy = object
                    .get("appId")
                    .or_else(|| object.get("appName"))
                    .and_then(Value::as_str)
                    .ok_or("INVALID_CANDIDATES")?;
                (Some(legacy.to_owned()), vec![legacy.to_owned()])
            };
            let registry = state.registry.lock().map_err(|_| "EXECUTION_FAILED")?;
            let target = resolve_application(&registry, canonical.as_deref(), &terms)?;
            native_windows::control_window(&target.process_name, tool)?;
            Ok(ok(
                tool,
                json!({"appId":normalized(&target.name),"displayName":target.name}),
            ))
        }
        "file.search" | "folder.search" => {
            let o = args(arguments, &["directory", "query", "extensions", "limit"])?;
            let directory_value = o.get("directory").ok_or("INVALID_ARGUMENT")?;
            let directory = if let Some(legacy) = directory_value.as_str() {
                legacy
            } else {
                resolve_folder(directory_value)?
            };
            let query_value = o.get("query").ok_or("INVALID_ARGUMENT")?;
            let queries = if let Some(legacy) = query_value.as_str() {
                vec![legacy.to_owned()]
            } else {
                query_reference(query_value)?
            };
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
            if exts.len() > 10
                || exts.iter().any(|ext| {
                    ext.is_empty()
                        || ext.len() > 10
                        || !ext.chars().all(|c| c.is_ascii_alphanumeric())
                })
            {
                return Err("INVALID_ARGUMENT");
            }
            let limit = o
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(20)
                .clamp(1, 50) as usize;
            let values = search(directory, &queries, &exts, tool == "folder.search", limit)?;
            let results = values
                .into_iter()
                .map(|hit| {
                    let resource_id =
                        register_resource(tool_state, hit.path, tool == "folder.search")?;
                    Ok(json!({"resourceId":resource_id,"displayName":hit.name,"score":hit.score}))
                })
                .collect::<Result<Vec<Value>, &'static str>>()?;
            Ok(ok(tool, json!({"results":results})))
        }
        "file.open" => {
            let o = args(arguments, &["resourceId", "directory", "path"])?;
            let path = if let Some(id) = o.get("resourceId").and_then(Value::as_str) {
                resource(tool_state, id, false)?
            } else {
                existing(string(o, "directory")?, string(o, "path")?, false)?
            };
            native_windows::open_with_shell(&path)?;
            Ok(ok(
                tool,
                json!({"opened":true,"displayName":path.file_name().and_then(|v|v.to_str())}),
            ))
        }
        "folder.open" => {
            let o = args(
                arguments,
                &[
                    "canonicalId",
                    "original",
                    "candidates",
                    "resourceId",
                    "directory",
                    "path",
                ],
            )?;
            let path = if let Some(id) = o.get("resourceId").and_then(Value::as_str) {
                resource(tool_state, id, true)?
            } else if o.contains_key("original") {
                root(resolve_folder(arguments)?)?
            } else if let Some(v) = o.get("path").and_then(Value::as_str) {
                existing(string(o, "directory")?, v, true)?
            } else {
                root(string(o, "directory")?)?
            };
            native_windows::open_with_shell(&path)?;
            Ok(ok(tool, json!({"opened":true})))
        }
        "folder.create" => {
            let o = args(arguments, &["parent", "name"])?;
            let parent_value = o.get("parent").ok_or("INVALID_ARGUMENT")?;
            let parent = if let Some(value) = parent_value.as_str() {
                value
            } else {
                resolve_folder(parent_value)?
            };
            let path = destination(parent, single_name(string(o, "name")?)?)?;
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
            let o = args(arguments, &["resourceId", "directory", "path", "newName"])?;
            let source = if let Some(id) = o.get("resourceId").and_then(Value::as_str) {
                resource(tool_state, id, false)?
            } else {
                existing(string(o, "directory")?, string(o, "path")?, false)?
            };
            let new_name = single_name(string(o, "newName")?)?;
            let target = source.parent().ok_or("PATH_NOT_ALLOWED")?.join(new_name);
            if target.exists() {
                return Err("DESTINATION_EXISTS");
            }
            fs::rename(source, &target).map_err(|_| "PERMISSION_DENIED")?;
            let resource_id = register_resource(tool_state, target.clone(), false)?;
            Ok(ok(
                tool,
                json!({"renamed":true,"resourceId":resource_id,"displayName":target.file_name().and_then(|v|v.to_str())}),
            ))
        }
        "file.move" | "file.copy" => {
            let o = args(
                arguments,
                &[
                    "resourceId",
                    "sourceDirectory",
                    "sourcePath",
                    "destinationDirectory",
                    "destinationPath",
                ],
            )?;
            let source = if let Some(id) = o.get("resourceId").and_then(Value::as_str) {
                resource(tool_state, id, false)?
            } else {
                existing(
                    string(o, "sourceDirectory")?,
                    string(o, "sourcePath")?,
                    false,
                )?
            };
            let destination_value = o.get("destinationDirectory").ok_or("INVALID_ARGUMENT")?;
            let destination_directory = if let Some(value) = destination_value.as_str() {
                value
            } else {
                resolve_folder(destination_value)?
            };
            let target = destination(destination_directory, string(o, "destinationPath")?)?;
            if tool == "file.copy" {
                fs::copy(source, &target).map_err(|_| "PERMISSION_DENIED")?;
            } else {
                fs::rename(source, &target).map_err(|_| "PERMISSION_DENIED")?;
            }
            let resource_id = register_resource(tool_state, target.clone(), false)?;
            Ok(ok(
                tool,
                json!({"completed":true,"resourceId":resource_id,"displayName":target.file_name().and_then(|v|v.to_str())}),
            ))
        }
        "file.delete" => {
            let o = args(arguments, &["resourceId", "directory", "path"])?;
            let path = if let Some(id) = o.get("resourceId").and_then(Value::as_str) {
                resource(tool_state, id, false)?
            } else {
                existing(string(o, "directory")?, string(o, "path")?, false)?
            };
            native_windows::recycle(&path)?;
            Ok(ok(tool, json!({"recycled":true})))
        }
        "web.open" => {
            let o = args(arguments, &["url", "original", "candidates"])?;
            if o.contains_key("original") {
                let original = string(o, "original")?;
                let candidates = o
                    .get("candidates")
                    .and_then(Value::as_array)
                    .ok_or("INVALID_CANDIDATES")?
                    .iter()
                    .map(|v| v.as_str().map(str::to_owned).ok_or("INVALID_CANDIDATES"))
                    .collect::<Result<Vec<_>, _>>()?;
                let _ = validated_terms(original, &candidates)?;
            }
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
            let resource_id = register_resource(tool_state, path.clone(), false)?;
            Ok(ok(
                tool,
                json!({"resourceId":resource_id,"displayName":path.file_name().and_then(|v|v.to_str())}),
            ))
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
    fn app(name: &str, aliases: &[&str]) -> InstalledApp {
        InstalledApp {
            name: name.into(),
            aliases: aliases.iter().map(|v| (*v).into()).collect(),
            executable_path: std::env::current_exe().unwrap(),
            process_name: "app.exe".into(),
            app_type: "desktop".into(),
            last_verified: 0,
        }
    }
    #[test]
    fn application_resolution_uses_canonical_name_alias_and_candidate_fallback() {
        let registry = vec![
            app("Visual Studio Code", &["vscode", "VS Code", "브이에스코드"]),
            app("Discord", &["discord", "디스코드", "디코"]),
            app("KakaoTalk", &["kakaotalk", "카톡"]),
        ];
        assert_eq!(
            resolve_application(&registry, Some("vscode"), &["브이에스코드".into()])
                .unwrap()
                .name,
            "Visual Studio Code"
        );
        assert_eq!(
            resolve_application(&registry, None, &["디코".into()])
                .unwrap()
                .name,
            "Discord"
        );
        assert_eq!(
            resolve_application(&registry, None, &["카톡".into()])
                .unwrap()
                .name,
            "KakaoTalk"
        );
        assert_eq!(
            resolve_application(&registry, None, &["없는 앱".into()]).unwrap_err(),
            "APP_NOT_FOUND"
        );
    }
    #[test]
    fn ambiguous_and_invalid_candidates_never_execute() {
        let registry = vec![app("Code Alpha", &["code"]), app("Code Beta", &["code"])];
        assert_eq!(
            resolve_application(&registry, None, &["code".into()]).unwrap_err(),
            "AMBIGUOUS_APP_MATCH"
        );
        assert_eq!(validated_terms("", &[]).unwrap_err(), "INVALID_CANDIDATES");
        assert_eq!(
            validated_terms("앱", &vec!["x".into(); 6]).unwrap_err(),
            "INVALID_CANDIDATES"
        );
    }
    #[test]
    fn known_folder_candidates_resolve_in_korean_and_english() {
        assert_eq!(
            resolve_folder(
                &json!({"canonicalId":"desktop","original":"바탕화면","candidates":["Desktop"]})
            )
            .unwrap(),
            "desktop"
        );
        assert_eq!(
            resolve_folder(&json!({"original":"다운로드","candidates":["downloads"]})).unwrap(),
            "downloads"
        );
    }
    #[test]
    fn file_search_ranks_candidates_deduplicates_and_filters_extensions() {
        let directory = std::env::temp_dir().join(format!("ace-native-search-{}", timestamp()));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("포트폴리오.pdf"), b"pdf").unwrap();
        fs::write(directory.join("portfolio.txt"), b"text").unwrap();
        let hits = search_in(
            &directory,
            &["포폴".into(), "포트폴리오".into(), "portfolio".into()],
            &["pdf".into()],
            false,
            20,
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "포트폴리오.pdf");
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn resource_id_supports_search_then_action_and_expires() {
        let path = std::env::temp_dir().join(format!("ace-resource-{}.txt", timestamp()));
        fs::write(&path, b"ace").unwrap();
        let state = NativeToolState::default();
        let id = register_resource(&state, path.clone(), false).unwrap();
        assert_eq!(resource(&state, &id, false).unwrap(), path);
        state
            .resources
            .lock()
            .unwrap()
            .get_mut(&id)
            .unwrap()
            .expires_at = Instant::now() - Duration::from_secs(1);
        assert_eq!(
            resource(&state, &id, false).unwrap_err(),
            "RESOURCE_EXPIRED"
        );
        fs::remove_file(path).unwrap();
    }
}
