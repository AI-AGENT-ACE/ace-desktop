use crate::{
    local_commands::{InstalledApp, NativeExecutionState},
    native_windows, voice_overlay,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
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

fn resource_log(message: impl std::fmt::Display) {
    if cfg!(debug_assertions) {
        eprintln!("{message}");
    }
}

fn loggable_resource_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

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
    identity: String,
    search_only: bool,
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
    #[serde(rename = "attemptId")]
    attempt_id: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    #[serde(rename = "executionId")]
    execution_id: Option<String>,
    status: String,
    success: bool,
    tool: String,
    data: Option<Value>,
    error: Option<NativeToolError>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditLog<'a> {
    attempt_id: Option<&'a str>,
    request_id: Option<&'a str>,
    execution_id: Option<&'a str>,
    tool: &'a str,
    risk: Risk,
    status: &'a str,
    duration_ms: u128,
    error_code: Option<&'a str>,
    timestamp_ms: u128,
}

fn settings_page(o: &serde_json::Map<String, Value>) -> Result<&str, &'static str> {
    let page = match o.get("page") {
        None => "",
        Some(Value::String(page)) => page.as_str(),
        _ => return Err("INVALID_ARGUMENT"),
    };
    if [
        "",
        "display",
        "sound",
        "bluetooth",
        "network",
        "appsfeatures",
        "privacy-microphone",
        "privacy-webcam",
        "windowsupdate",
        "personalization",
        "notifications",
        "about",
    ]
    .contains(&page)
    {
        Ok(page)
    } else {
        Err("INVALID_ARGUMENT")
    }
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
        | "clipboard.read"
        | "voice.activate"
        | "screen.capture"
        | "system.volume.set"
        | "system.volume.mute"
        | "system.open_settings" => Risk::Safe,
        "app.close" | "file.rename" | "file.move" | "file.copy" | "file.delete"
        | "folder.create" | "clipboard.write" | "system.lock" => Risk::Confirm,
        _ => Risk::Blocked,
    }
}

fn ok(tool: &str, data: Value) -> NativeToolResult {
    NativeToolResult {
        request_id: None,
        execution_id: None,
        attempt_id: None,
        status: "SUCCEEDED".into(),
        success: true,
        tool: tool.into(),
        data: Some(data),
        error: None,
    }
}
fn fail(tool: &str, code: &str) -> NativeToolResult {
    let message = match code {
        "APP_NOT_FOUND" => "설치된 앱을 찾지 못했습니다.",
        "FILE_OVERWRITE_CONFIRMATION_REQUIRED" => {
            "같은 이름의 파일이 있습니다. 덮어쓰기 여부를 확인해 주세요."
        }
        "FILE_CONFIRMATION_STALE" => "파일 또는 승인 정보가 변경되었습니다. 다시 확인해 주세요.",
        "SAFE_OVERWRITE_UNAVAILABLE" => {
            "이 위치에서는 안전한 교체를 보장할 수 없습니다. 다른 이름이나 위치를 선택해 주세요."
        }
        "RECYCLE_UNAVAILABLE" => "휴지통으로 이동할 수 없습니다. 파일은 영구 삭제하지 않았습니다.",
        "CAPTURE_SELECTION_REQUIRED" => "캡처할 모니터와 범위를 선택해 주세요.",
        "CAPTURE_MONITOR_DISCONNECTED" => {
            "선택한 모니터의 연결 또는 배치가 변경되었습니다. 다시 선택해 주세요."
        }
        "CAPTURE_CANCELLED" => "화면 캡처를 취소했습니다.",
        "WINDOW_NOT_FOUND" => "실행 중인 앱 창을 찾지 못했습니다.",
        "WINDOW_SELECTION_REQUIRED" => "작업할 창을 선택해 주세요.",
        "WINDOW_TARGET_STALE" => "선택한 창이 닫혔거나 변경되었습니다. 다시 선택해 주세요.",
        "FILE_NOT_FOUND" => "파일을 찾지 못했습니다.",
        "DIRECTORY_NOT_FOUND" => "폴더를 찾지 못했습니다.",
        "PATH_NOT_ALLOWED" => "허용된 위치 밖의 경로입니다.",
        "DESTINATION_EXISTS" => "대상 위치에 같은 이름이 이미 있습니다.",
        "CONFIRMATION_REQUIRED" => "사용자 확인이 필요합니다.",
        "BLOCKED_BY_POLICY" => "보안 정책으로 차단된 기능입니다.",
        "CLIPBOARD_DENIED" => "클립보드에 접근할 수 없습니다.",
        "CLIPBOARD_UNSUPPORTED_FORMAT" => "클립보드에 읽을 수 있는 텍스트가 없습니다.",
        "MICROPHONE_DISCONNECTED" => {
            "선택한 마이크가 연결되어 있지 않습니다. 설정에서 마이크를 선택해 주세요."
        }
        "MICROPHONE_UNAVAILABLE" => {
            "마이크에 접근할 수 없습니다. 장치와 Windows 마이크 권한을 확인해 주세요."
        }
        "NATIVE_APPROVAL_STALE" => {
            "승인이 만료되었거나 실행 대상이 변경되었습니다. 다시 확인해 주세요."
        }
        "DUPLICATE_EXECUTION" => "이미 처리한 실행 요청입니다.",
        "SCREEN_CAPTURE_FAILED" => "화면을 캡처하지 못했습니다.",
        "WINDOW_FOCUS_FAILED" => {
            "대상 창을 앞으로 가져오지 못해 종료 요청을 보내지 않았습니다.\n창을 직접 확인한 뒤 다시 시도해 주세요."
        }
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
        request_id: None,
        execution_id: None,
        attempt_id: None,
        status: if matches!(
            code,
            "WINDOW_SELECTION_REQUIRED" | "CAPTURE_SELECTION_REQUIRED"
        ) {
            "SELECTION_REQUIRED"
        } else if code == "FILE_OVERWRITE_CONFIRMATION_REQUIRED" || code == "CONFIRMATION_REQUIRED"
        {
            "APPROVAL_REQUIRED"
        } else if code == "CAPTURE_CANCELLED" {
            "CANCELLED"
        } else {
            "FAILED"
        }
        .into(),
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
    // Windows may register both the system launcher and Store Notepad. Prefer only
    // the trusted built-in launcher for an exact Notepad request, never an arbitrary tie.
    if canonical
        .into_iter()
        .chain(terms.iter().map(String::as_str))
        .any(|term| {
            matches!(
                normalized(term).as_str(),
                "notepad" | "windowsnotepad" | "메모장"
            )
        })
    {
        if let Some(windows) = std::env::var_os("WINDIR") {
            let expected = std::path::PathBuf::from(windows)
                .join("System32")
                .join("notepad.exe");
            let normalize_path = |path: &std::path::Path| {
                path.to_string_lossy()
                    .replace('/', "\\")
                    .trim_start_matches(r"\\?\")
                    .to_ascii_lowercase()
            };
            if normalize_path(&app.executable_path) == normalize_path(&expected) {
                return 110;
            }
        }
    }
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
        || value.contains(':')
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
        || value.encode_utf16().count() > 255
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
        || value.ends_with('.')
        || value
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
    {
        return Err("INVALID_ARGUMENT");
    }
    let stem = value.split('.').next().unwrap_or("").to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                ["1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³"].contains(&suffix)
            })
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
fn allowed_absolute_file(value: &str) -> Result<PathBuf, &'static str> {
    let requested = Path::new(value);
    if !requested.is_absolute() {
        return Err("INVALID_ARGUMENT");
    }
    if requested.components().any(|component|matches!(component,Component::Normal(value) if value.to_string_lossy().contains(':'))){return Err("PATH_NOT_ALLOWED");}
    let metadata = fs::symlink_metadata(requested).map_err(|_| "FILE_NOT_FOUND")?;
    if metadata.file_type().is_symlink() {
        return Err("PATH_NOT_ALLOWED");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("PATH_NOT_ALLOWED");
        }
    }
    let path = requested.canonicalize().map_err(|_| "FILE_NOT_FOUND")?;
    let allowed = [
        "desktop",
        "downloads",
        "documents",
        "pictures",
        "music",
        "videos",
    ]
    .iter()
    .filter_map(|name| root(name).ok())
    .filter_map(|base| base.canonicalize().ok())
    .any(|base| path.starts_with(base));
    if !allowed {
        return Err("PATH_NOT_ALLOWED");
    }
    if !path.is_file() {
        return Err("FILE_NOT_FOUND");
    }
    Ok(path)
}
fn file_destination(directory: &str, value: &str) -> Result<PathBuf, &'static str> {
    let base = root(directory)?
        .canonicalize()
        .map_err(|_| "DIRECTORY_NOT_FOUND")?;
    let relative = relative(value)?;
    let candidate = base.join(&relative);
    let parent = candidate
        .parent()
        .ok_or("PATH_NOT_ALLOWED")?
        .canonicalize()
        .map_err(|_| "DIRECTORY_NOT_FOUND")?;
    if !parent.starts_with(&base) {
        return Err("PATH_NOT_ALLOWED");
    }
    let name = candidate
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("INVALID_ARGUMENT")?;
    Ok(parent.join(single_name(name)?))
}

fn transfer_destination(
    tool: &str,
    source: &Path,
    o: &Map<String, Value>,
) -> Result<PathBuf, &'static str> {
    if tool == "file.copy"
        && !o.contains_key("destinationDirectory")
        && !o.contains_key("destinationPath")
    {
        let parent = source.parent().ok_or("PATH_NOT_ALLOWED")?;
        let stem = source
            .file_stem()
            .and_then(|v| v.to_str())
            .ok_or("INVALID_ARGUMENT")?;
        let extension = source
            .extension()
            .and_then(|v| v.to_str())
            .map(|v| format!(".{v}"))
            .unwrap_or_default();
        for index in 1..=1000 {
            let suffix = if index == 1 {
                String::new()
            } else {
                format!(" ({index})")
            };
            let name = format!("{stem}의 복사본{suffix}{extension}");
            let target = parent.join(single_name(&name)?);
            if !target.exists() {
                return Ok(target);
            }
        }
        return Err("FILE_ALREADY_EXISTS");
    }
    file_destination(
        string(o, "destinationDirectory")?,
        string(o, "destinationPath")?,
    )
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
) -> Result<(Vec<SearchHit>, bool, usize), &'static str> {
    let base = search_root(directory)?
        .canonicalize()
        .map_err(|_| "DIRECTORY_NOT_FOUND")?;
    search_in_report(&base, queries, extensions, folders, limit)
}

fn search_root(directory: &str) -> Result<PathBuf, &'static str> {
    if directory == "drive_c" {
        #[cfg(windows)]
        return Ok(PathBuf::from("C:\\"));
        #[cfg(not(windows))]
        return Err("UNSUPPORTED_PLATFORM");
    }
    root(directory)
}

#[cfg(test)]
fn search_in(
    base: &Path,
    queries: &[String],
    extensions: &[String],
    folders: bool,
    limit: usize,
) -> Result<Vec<SearchHit>, &'static str> {
    search_in_report(base, queries, extensions, folders, limit).map(|result| result.0)
}
fn search_in_report(
    base: &Path,
    queries: &[String],
    extensions: &[String],
    folders: bool,
    limit: usize,
) -> Result<(Vec<SearchHit>, bool, usize), &'static str> {
    let base = base.canonicalize().map_err(|_| "DIRECTORY_NOT_FOUND")?;
    // An explicit relative path must not become a broad basename search.
    // Resolve it before the bounded recursive scan, checking every component.
    if let Some(query) = queries
        .first()
        .filter(|q| q.contains('/') || q.contains('\\'))
    {
        let relative_path = relative(query)?;
        let mut path = base.clone();
        for component in relative_path.components() {
            path.push(component);
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok((vec![], false, 0))
                }
                Err(_) => return Err("PERMISSION_DENIED"),
            };
            if metadata.file_type().is_symlink() {
                return Err("PATH_NOT_ALLOWED");
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err("PATH_NOT_ALLOWED");
                }
            }
        }
        let path = path.canonicalize().map_err(|_| "FILE_NOT_FOUND")?;
        if !path.starts_with(&base) {
            return Err("PATH_NOT_ALLOWED");
        }
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if path.is_dir() != folders
            || (!folders && !path.is_file())
            || (!folders && !extensions.is_empty() && !extensions.contains(&extension))
        {
            return Ok((vec![], false, 0));
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        return Ok((
            vec![SearchHit {
                path,
                name,
                score: 100,
            }],
            false,
            0,
        ));
    }
    let mut stack = VecDeque::from([base.clone()]);
    // Search the user's area first, then the rest of C:, without visiting twice.
    if let Some(home) = dirs::home_dir().and_then(|p| p.canonicalize().ok()) {
        if home.starts_with(&base) && home != base {
            stack.push_front(home);
        }
    }
    let mut visited = HashSet::new();
    let mut found = Vec::new();
    let mut scanned = 0usize;
    let started = Instant::now();
    let mut partial = false;
    let mut skipped = 0usize;
    'scan: while let Some(current) = stack.pop_front() {
        if !visited.insert(current.clone()) {
            continue;
        }
        let entries = match fs::read_dir(current) {
            Ok(entries) => entries,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        for entry in entries.flatten() {
            scanned += 1;
            if scanned > 1_000_000 || started.elapsed() > Duration::from_secs(20) {
                partial = true;
                break 'scan;
            }
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    continue;
                }
            }
            let is_dir = path.is_dir();
            if is_dir {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                // Defer large system/build trees, but do not exclude them.
                if [
                    "windows",
                    "program files",
                    "program files (x86)",
                    "programdata",
                    "appdata",
                    "node_modules",
                    "target",
                    ".git",
                    ".cache",
                    "dist",
                    "build",
                ]
                .contains(&name.as_str())
                {
                    stack.push_back(path.clone());
                } else {
                    stack.push_front(path.clone());
                }
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
    }
    found.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.name.cmp(&right.name))
    });
    found.truncate(limit);
    Ok((found, partial, skipped))
}

pub(crate) fn register_resource(
    state: &NativeToolState,
    path: PathBuf,
    directory: bool,
) -> Result<String, &'static str> {
    use std::hash::{Hash, Hasher};
    let path = path.canonicalize().map_err(|_| "RESOURCE_NOT_FOUND")?;
    let identity = resource_identity(&path)?;
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
            identity,
            search_only: false,
        },
    );
    resource_log(format!("[Resource] issued id={id}"));
    Ok(id)
}

fn resource_identity(path: &Path) -> Result<String, &'static str> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "RESOURCE_NOT_FOUND")?;
    if metadata.file_type().is_symlink() {
        return Err("RESOURCE_NOT_FOUND");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("RESOURCE_NOT_FOUND");
        }
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION},
        };
        let handle = OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x02000000)
            .open(path)
            .map_err(|_| "RESOURCE_NOT_FOUND")?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(HANDLE(handle.as_raw_handle()), &mut info) }
            .map_err(|_| "RESOURCE_NOT_FOUND")?;
        Ok(format!(
            "{}:{}:{}:{}:{:?}",
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
            metadata.len(),
            metadata.modified().ok()
        ))
    }
    #[cfg(not(windows))]
    {
        Ok(format!("{}:{:?}", metadata.len(), metadata.modified().ok()))
    }
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
    if entry.search_only {
        return Err("PATH_NOT_ALLOWED");
    }
    if entry.directory != directory
        || !entry.path.exists()
        || resource_identity(&entry.path)? != entry.identity
    {
        return Err("RESOURCE_NOT_FOUND");
    }
    if loggable_resource_id(id) {
        resource_log(format!("[Validation] resource_id={id} result=passed"));
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
        attempt_id: result.attempt_id.as_deref(),
        request_id: result.request_id.as_deref(),
        execution_id: result.execution_id.as_deref(),
        tool,
        risk,
        status: result.status.as_str(),
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

// Tauri injects four runtime parameters; keep the established IPC contract.
fn approval_snapshot(
    state: &NativeToolState,
    tool: &str,
    arguments: &Value,
) -> Result<String, &'static str> {
    if tool == "folder.create" {
        let o = args(
            arguments,
            &[
                "parent",
                "parentResourceId",
                "parentDirectory",
                "parentPath",
                "name",
            ],
        )?;
        let parent = folder_parent(state, o)?;
        return Ok(format!(
            "{}|{}",
            resource_identity(&parent)?,
            single_name(string(o, "name")?)?
        ));
    }
    if !matches!(
        tool,
        "file.copy" | "file.move" | "file.rename" | "file.delete"
    ) {
        return Ok(String::new());
    }
    let o = arguments.as_object().ok_or("INVALID_ARGUMENT")?;
    let source = if let Some(id) = o.get("resourceId").and_then(Value::as_str) {
        resource(state, id, false)?
    } else if matches!(tool, "file.copy" | "file.move") {
        existing(
            string(o, "sourceDirectory")?,
            string(o, "sourcePath")?,
            false,
        )?
    } else if o.contains_key("directory") {
        existing(string(o, "directory")?, string(o, "path")?, false)?
    } else {
        allowed_absolute_file(string(o, "path")?)?
    };
    let mut snapshot = crate::file_operations::approval_fingerprint(&source)?;
    if tool != "file.delete" {
        let target = if tool == "file.rename" {
            source
                .parent()
                .ok_or("PATH_NOT_ALLOWED")?
                .join(single_name(string(o, "newName")?)?)
        } else {
            transfer_destination(tool, &source, o)?
        };
        snapshot.push_str(&format!("|{}|", target.to_string_lossy()));
        if target.exists() {
            snapshot.push_str(&crate::file_operations::approval_fingerprint(&target)?);
        } else {
            snapshot.push_str("absent");
        }
    }
    Ok(snapshot)
}
fn folder_parent(state: &NativeToolState, o: &Map<String, Value>) -> Result<PathBuf, &'static str> {
    if let Some(id) = o.get("parentResourceId").and_then(Value::as_str) {
        resource(state, id, true)
    } else if o.contains_key("parentPath") {
        existing(
            string(o, "parentDirectory")?,
            string(o, "parentPath")?,
            true,
        )
    } else {
        let value = o.get("parent").ok_or("INVALID_ARGUMENT")?;
        root(if let Some(name) = value.as_str() {
            name
        } else {
            resolve_folder(value)?
        })
    }
}
#[tauri::command]
pub async fn await_voice_activation(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Value, String> {
    if window.label() != "main" {
        return Err("BLOCKED_BY_POLICY".into());
    }
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(12) {
        if !voice_overlay::is_active(&app) {
            return Err("CANCELLED".into());
        }
        match crate::audio_input::voice_readiness(&app) {
            Ok(true) => return Ok(json!({"state":"LISTENING"})),
            Err(e) => return Err(e.into()),
            _ => {}
        }
        tauri::async_runtime::spawn_blocking(|| std::thread::sleep(Duration::from_millis(50)))
            .await
            .map_err(|_| "MICROPHONE_UNAVAILABLE")?;
    }
    Err("MICROPHONE_START_TIMEOUT".into())
}
#[tauri::command]
pub fn authorize_native_tool(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    pending_id: String,
) -> Result<String, String> {
    if window.label() != "main" {
        return Err("BLOCKED_BY_POLICY".into());
    }
    let approvals = app.state::<crate::native_approvals::Approvals>();
    let (tool, arguments) = approvals
        .inspect(&pending_id, window.label())
        .map_err(str::to_owned)?;
    let snapshot = prepared_snapshot(&app, &tool, &arguments).map_err(str::to_owned)?;
    approvals
        .approve(&pending_id, window.label(), &snapshot)
        .map_err(str::to_owned)
}
fn prepared_snapshot(
    app: &tauri::AppHandle,
    tool: &str,
    arguments: &Value,
) -> Result<String, &'static str> {
    match tool {
        "file.copy" | "file.move" => {
            args(
                arguments,
                &[
                    "resourceId",
                    "sourceDirectory",
                    "sourcePath",
                    "destinationDirectory",
                    "destinationPath",
                    "overwriteToken",
                ],
            )?;
        }
        "file.rename" => {
            args(
                arguments,
                &[
                    "resourceId",
                    "directory",
                    "path",
                    "newName",
                    "overwriteToken",
                ],
            )?;
        }
        "file.delete" => {
            args(arguments, &["resourceId", "directory", "path"])?;
        }
        "app.close" => {
            args(
                arguments,
                &[
                    "appName",
                    "appId",
                    "canonicalId",
                    "original",
                    "candidates",
                    "targetWindowId",
                ],
            )?;
        }
        _ => {}
    }
    if tool == "app.close" {
        return crate::app_windows::approval_snapshot(
            &app.state::<crate::app_windows::WindowTargets>(),
            arguments
                .get("targetWindowId")
                .and_then(Value::as_str)
                .ok_or("WINDOW_SELECTION_REQUIRED")?,
        );
    }
    if matches!(tool, "system.lock" | "clipboard.read") {
        args(arguments, &[])?;
    }
    if tool == "clipboard.write" {
        let o = args(arguments, &["text"])?;
        let text = o
            .get("text")
            .and_then(Value::as_str)
            .ok_or("INVALID_ARGUMENT")?;
        if text.len() > MAX_CLIPBOARD || text.contains('\0') {
            return Err("INVALID_ARGUMENT");
        }
    }
    approval_snapshot(&app.state::<NativeToolState>(), tool, arguments)
}
fn single_close_target(result: &NativeToolResult) -> Option<&str> {
    if result.error.as_ref()?.code != "WINDOW_SELECTION_REQUIRED" {
        return None;
    }
    let choices = result.data.as_ref()?.get("choices")?.as_array()?;
    if choices.len() != 1 {
        return None;
    }
    choices[0]
        .get("targetId")?
        .as_str()
        .filter(|id| !id.is_empty())
}

#[tauri::command]
pub fn prepare_native_tool(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    tool: String,
    arguments: Value,
    execution_id: String,
    attempt_id: String,
) -> NativeToolResult {
    if window.label() != "main" || policy(&tool) != Risk::Confirm {
        return fail(&tool, "BLOCKED_BY_POLICY");
    }
    let mut arguments = arguments;
    if tool == "app.close" && arguments.get("targetWindowId").is_none() {
        let selection = dispatch(
            &app,
            &app.state::<NativeExecutionState>(),
            &app.state::<NativeToolState>(),
            &tool,
            &arguments,
            None,
        )
        .unwrap_or_else(|code| fail(&tool, code));
        if let Some(id) = single_close_target(&selection) {
            arguments["targetWindowId"] = Value::String(id.to_owned());
        } else {
            return selection;
        }
    }
    let result = (|| {
        let snapshot = prepared_snapshot(&app, &tool, &arguments)?;
        let pending_id = app.state::<crate::native_approvals::Approvals>().prepare(
            window.label(),
            &execution_id,
            &attempt_id,
            &tool,
            &arguments,
            snapshot,
        )?;
        let (stored_tool, stored_args) = app
            .state::<crate::native_approvals::Approvals>()
            .inspect(&pending_id, window.label())?;
        let view = approval_view(&app, &stored_tool, &stored_args)?;
        let mut description = view["question"]
            .as_str()
            .unwrap_or("작업을 진행할까요?")
            .to_string();
        if let Some(targets) = view["targets"].as_array() {
            for target in targets {
                description.push_str(&format!(
                    "\n{}: {}",
                    target["label"].as_str().unwrap_or("대상"),
                    target["path"]
                        .as_str()
                        .or_else(|| target["name"].as_str())
                        .unwrap_or("")
                ));
            }
        }
        if let Some(warning) = view["warning"].as_str().filter(|v| !v.is_empty()) {
            description.push_str(&format!("\n{warning}"));
        }
        let mut result = fail(&tool, "NATIVE_APPROVAL_REQUIRED");
        result.data = Some(
            json!({"pendingId": pending_id, "executionId": execution_id, "attemptId": attempt_id, "description": description, "view":view, "preparedArguments": stored_args}),
        );
        Ok(result)
    })();
    result.unwrap_or_else(|code| fail(&tool, code))
}
fn approval_view(
    app: &tauri::AppHandle,
    tool: &str,
    arguments: &Value,
) -> Result<Value, &'static str> {
    let o = arguments.as_object().ok_or("INVALID_ARGUMENT")?;
    if tool == "app.close" {
        let label = crate::app_windows::approval_label(
            &app.state::<crate::app_windows::WindowTargets>(),
            string(o, "targetWindowId")?,
        )?;
        return Ok(
            json!({"question":"이 창을 닫을까요?","action":"닫기","targets":[{"label":"선택한 창","name":label}],"warning":"저장 확인이 나오면 직접 처리해 주세요."}),
        );
    }
    if tool == "system.lock" {
        return Ok(json!({"question":"컴퓨터를 잠글까요?","action":"잠금","targets":[]}));
    }
    if tool == "clipboard.read" {
        return Ok(
            json!({"question":"복사한 텍스트를 읽을까요?","action":"읽기","targets":[],"warning":"채팅 요청으로 읽은 내용은 AI에 전달됩니다."}),
        );
    }
    if tool == "clipboard.write" {
        let text = o
            .get("text")
            .and_then(Value::as_str)
            .ok_or("INVALID_ARGUMENT")?;
        return Ok(
            json!({"question":"이 텍스트를 클립보드에 저장할까요?","action":"저장","targets":[{"label":"저장할 내용","name":text.chars().take(300).collect::<String>()}],"warning":"기존 클립보드 내용이 바뀝니다."}),
        );
    }
    let state = app.state::<NativeToolState>();
    if tool == "folder.create" {
        let path = folder_parent(&state, o)?.join(single_name(string(o, "name")?)?);
        return Ok(
            json!({"question":"이 위치에 폴더를 만들까요?","action":"만들기","targets":[approval_path("대상", &path)]}),
        );
    }
    let source = if let Some(id) = o.get("resourceId").and_then(Value::as_str) {
        resource(&state, id, false)?
    } else if matches!(tool, "file.copy" | "file.move") {
        existing(
            string(o, "sourceDirectory")?,
            string(o, "sourcePath")?,
            false,
        )?
    } else if o.contains_key("directory") {
        existing(string(o, "directory")?, string(o, "path")?, false)?
    } else {
        allowed_absolute_file(string(o, "path")?)?
    };
    if tool == "file.delete" {
        return Ok(
            json!({"question":"이 파일을 휴지통으로 옮길까요?","action":"휴지통으로 이동","targets":[approval_path("대상", &source)],"warning":"휴지통에서 복원할 수 있습니다."}),
        );
    }
    let target = if tool == "file.rename" {
        source
            .parent()
            .ok_or("PATH_NOT_ALLOWED")?
            .join(single_name(string(o, "newName")?)?)
    } else {
        transfer_destination(tool, &source, o)?
    };
    let action = match tool {
        "file.copy" => "복사",
        "file.move" => "이동",
        _ => "이름 변경",
    };
    let overwrite = o.contains_key("overwriteToken");
    Ok(
        json!({"question":if overwrite { "이 파일을 덮어쓸까요?".to_string() } else { format!("이 파일을 {action}할까요?") },
      "action":if overwrite {"덮어쓰기"} else {action},
      "targets":[approval_path("원본", &source),approval_path("대상", &target)],
      "warning":if overwrite {"대상 파일의 기존 내용이 바뀝니다."} else {""}}),
    )
}
#[tauri::command]
pub fn describe_native_target(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    tool: String,
    arguments: Value,
) -> Result<Value, String> {
    if window.label() != "main" {
        return Err("BLOCKED_BY_POLICY".into());
    }
    if tool == "app.open" {
        let o = args(
            &arguments,
            &["canonicalId", "original", "candidates", "appId", "appName"],
        )?;
        let (canonical, terms) = if o.contains_key("original") {
            entity(&arguments)?
        } else {
            let name = o
                .get("appId")
                .or_else(|| o.get("appName"))
                .or_else(|| o.get("canonicalId"))
                .and_then(Value::as_str)
                .ok_or("INVALID_CANDIDATES")?;
            (Some(name.to_string()), vec![name.to_string()])
        };
        let state = app.state::<NativeExecutionState>();
        let registry = state.registry.lock().map_err(|_| "EXECUTION_FAILED")?;
        let target = resolve_application(&registry, canonical.as_deref(), &terms)?;
        return Ok(
            json!({"question":"이 앱을 열까요?","action":"열기","targets":[{"label":"대상 앱","name":target.name,"path":target.executable_path.to_string_lossy()}]}),
        );
    }
    if tool != "file.open" {
        if tool == "clipboard.read" {
            args(&arguments, &[])?;
            return Ok(
                json!({"question":"복사한 텍스트를 읽을까요?","action":"읽기","targets":[],"warning":"채팅 요청으로 읽은 내용은 AI에 전달됩니다."}),
            );
        }
        return Err("BLOCKED_BY_POLICY".into());
    }
    let o = args(&arguments, &["resourceId", "directory", "path"])?;
    let path = if let Some(id) = o.get("resourceId").and_then(Value::as_str) {
        resource(&app.state::<NativeToolState>(), id, false)?
    } else if !o.contains_key("directory") {
        allowed_absolute_file(string(o, "path")?)?
    } else {
        existing(string(o, "directory")?, string(o, "path")?, false)?
    };
    Ok(
        json!({"question":"이 파일을 열까요?","action":"열기","targets":[approval_path("대상", &path)]}),
    )
}
fn approval_path(label: &str, path: &Path) -> Value {
    json!({"label":label,"name":path.file_name().unwrap_or_default().to_string_lossy(),"path":path.to_string_lossy()})
}
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn execute_native_tool(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    version: Option<String>,
    tool: String,
    arguments: Value,
    confirmed: bool,
    approval_id: Option<String>,
    execution_id: String,
    attempt_id: String,
    request_id: Option<String>,
) -> NativeToolResult {
    let state = app.state::<NativeExecutionState>();
    let tool_state = app.state::<NativeToolState>();
    let started = Instant::now();
    let _activity = match crate::activity::begin() {
        Ok(guard) => guard,
        Err(code) => return fail(&tool, code),
    };
    let risk = policy(&tool);
    let requested_resource_id = arguments
        .get("resourceId")
        .and_then(Value::as_str)
        .filter(|id| loggable_resource_id(id));
    let mut result = if version.as_deref().is_some_and(|value| value != "1.1") {
        fail(&tool, "INVALID_ARGUMENT")
    } else if window.label() != "main"
        || risk == Risk::Blocked
        || uuid::Uuid::parse_str(&execution_id).is_err()
    {
        fail(&tool, "BLOCKED_BY_POLICY")
    } else if risk == Risk::Confirm && (!confirmed || approval_id.is_none()) {
        fail(&tool, "CONFIRMATION_REQUIRED")
    } else {
        if let Some(id) = requested_resource_id {
            resource_log(format!("[Tool] received resource_id={id}"));
        }
        let authorities = app.state::<crate::native_approvals::Approvals>();
        let validation = authorities.start(&attempt_id).and_then(|_| {
            if risk == Risk::Confirm {
                let snapshot = prepared_snapshot(&app, &tool, &arguments)?;
                authorities.consume_bound(
                    approval_id.as_deref().unwrap(),
                    window.label(),
                    &execution_id,
                    &attempt_id,
                    &tool,
                    &arguments,
                    &snapshot,
                )
            } else {
                Ok(())
            }
        });
        if let Err(code) = validation {
            fail(&tool, code)
        } else if matches!(tool.as_str(), "file.search" | "folder.search") {
            let search_app = app.clone();
            let search_tool = tool.clone();
            let search_arguments = arguments.clone();
            tauri::async_runtime::spawn_blocking(move || {
                dispatch(
                    &search_app,
                    &search_app.state::<NativeExecutionState>(),
                    &search_app.state::<NativeToolState>(),
                    &search_tool,
                    &search_arguments,
                    None,
                )
            })
            .await
            .unwrap_or(Err("EXECUTION_FAILED"))
            .unwrap_or_else(|code| fail(&tool, code))
        } else if tool == "app.close" {
            // Activation belongs to the UI/input thread. Security validation and
            // one-use approval consumption have completed; dispatch rechecks the target.
            let (tx, rx) = tokio::sync::oneshot::channel();
            let ui_app = app.clone();
            let ui_arguments = arguments.clone();
            if app
                .run_on_main_thread(move || {
                    let result = dispatch(
                        &ui_app,
                        &ui_app.state::<NativeExecutionState>(),
                        &ui_app.state::<NativeToolState>(),
                        "app.close",
                        &ui_arguments,
                        None,
                    );
                    let _ = tx.send(result);
                })
                .is_err()
            {
                fail(&tool, "EXECUTION_FAILED")
            } else {
                rx.await
                    .unwrap_or(Err("EXECUTION_FAILED"))
                    .unwrap_or_else(|code| fail(&tool, code))
            }
        } else {
            dispatch(
                &app,
                &state,
                &tool_state,
                &tool,
                &arguments,
                Some(crate::screen_capture::Trace {
                    execution_id: execution_id.clone(),
                    attempt_id: attempt_id.clone(),
                }),
            )
            .unwrap_or_else(|code| fail(&tool, code))
        }
    };
    result.execution_id = uuid::Uuid::parse_str(&execution_id)
        .ok()
        .map(|_| execution_id);
    result.attempt_id = uuid::Uuid::parse_str(&attempt_id).ok().map(|_| attempt_id);
    if result.success
        && result.data.as_ref().is_some_and(|data| {
            data.get("closeTicket").is_some()
                || data.get("captureSessionId").is_some()
                || data.get("activationRequested").is_some()
        })
    {
        result.status = "REQUESTED".into();
    }
    if result.success {
        if let Some(id) = requested_resource_id {
            resource_log(format!("[Execution] resource_id={id} result=success"));
        }
    }
    result.request_id = request_id.filter(|id| uuid::Uuid::parse_str(id).is_ok());
    log(&app, &tool, risk, started, &result);
    result
}

#[cfg(test)]
mod overwrite_failure_contract_tests {
    #[test]
    fn overwrite_errors_are_failed_not_completed() {
        for tool in ["file.copy", "file.move"] {
            for code in ["EXECUTION_FAILED", "SAFE_OVERWRITE_UNAVAILABLE"] {
                let result = super::fail(tool, code);
                assert!(!result.success);
                assert_eq!(result.status, "FAILED");
                assert!(result.data.is_none());
            }
        }
    }
}

fn dispatch(
    app: &tauri::AppHandle,
    state: &NativeExecutionState,
    tool_state: &NativeToolState,
    tool: &str,
    arguments: &Value,
    capture_trace: Option<crate::screen_capture::Trace>,
) -> Result<NativeToolResult, &'static str> {
    match tool {
        "system.status" => {
            args(arguments, &[])?;
            Ok(ok(
                tool,
                json!({"os": std::env::consts::OS, "arch": std::env::consts::ARCH, "cpuThreads": std::thread::available_parallelism().map(|v|v.get()).unwrap_or(1)}),
            ))
        }
        "app.open" | "app.close" | "app.focus" | "app.minimize" | "app.maximize" => {
            let object = args(
                arguments,
                &[
                    "canonicalId",
                    "original",
                    "candidates",
                    "appId",
                    "appName",
                    "targetWindowId",
                ],
            )?;
            let (canonical, terms) = if object.contains_key("original") {
                let mut entity_args = arguments.clone();
                entity_args
                    .as_object_mut()
                    .unwrap()
                    .remove("targetWindowId");
                entity(&entity_args)?
            } else {
                let legacy = object
                    .get("appId")
                    .or_else(|| object.get("appName"))
                    .or_else(|| object.get("canonicalId"))
                    .and_then(Value::as_str)
                    .ok_or("INVALID_CANDIDATES")?;
                (Some(legacy.to_owned()), vec![legacy.to_owned()])
            };
            let registry = state.registry.lock().map_err(|_| "EXECUTION_FAILED")?;
            let target = resolve_application(&registry, canonical.as_deref(), &terms)?;
            if tool != "app.open" {
                let selected = object
                    .get("targetWindowId")
                    .map(|v| v.as_str().ok_or("INVALID_ARGUMENT"))
                    .transpose()?;
                match crate::app_windows::run(
                    &app.state::<crate::app_windows::WindowTargets>(),
                    &target.process_name,
                    &target.name,
                    tool,
                    selected,
                )? {
                    crate::app_windows::Outcome::Select(choices) => {
                        let mut result = fail(tool, "WINDOW_SELECTION_REQUIRED");
                        result.data = Some(json!({"choices":choices}));
                        return Ok(result);
                    }
                    crate::app_windows::Outcome::CloseRequested(ticket) => {
                        return Ok(ok(
                            tool,
                            json!({"displayName":target.name,"closeRequested":true,"closeTicket":ticket}),
                        ))
                    }
                    crate::app_windows::Outcome::Done => {
                        return Ok(ok(
                            tool,
                            json!({"displayName":target.name,"closeRequested":tool == "app.close"}),
                        ))
                    }
                }
            }
            if object.contains_key("targetWindowId") {
                return Err("INVALID_ARGUMENT");
            }
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
        "file.search" | "folder.search" => {
            let o = args(arguments, &["directory", "query", "extensions", "limit"])?;
            let default_directory = json!("drive_c");
            let directory_value = o.get("directory").unwrap_or(&default_directory);
            let directory = if let Some(legacy) = directory_value.as_str() {
                legacy
            } else if directory_value.get("canonicalId").and_then(Value::as_str) == Some("drive_c")
            {
                "drive_c"
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
                .map(|v| v.as_array().ok_or("INVALID_ARGUMENT"))
                .transpose()?
                .map(|a| {
                    a.iter()
                        .map(|s| s.as_str().map(str::to_owned).ok_or("INVALID_ARGUMENT"))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
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
                .map(|v| {
                    v.as_u64()
                        .filter(|n| (1..=50).contains(n))
                        .ok_or("INVALID_ARGUMENT")
                })
                .transpose()?
                .unwrap_or(20) as usize;
            let (values, partial, skipped) =
                search(directory, &queries, &exts, tool == "folder.search", limit)?;
            let base = search_root(directory)?
                .canonicalize()
                .map_err(|_| "PATH_NOT_ALLOWED")?;
            let results = values
                .into_iter()
                .filter_map(|mut hit| {
                    hit.path = hit.path.canonicalize().ok()?;
                    if !hit.path.starts_with(&base) { return None; }
                    let resource_id =
                        register_resource(tool_state, hit.path.clone(), tool == "folder.search").ok()?;
                    let relative_path = hit.path.strip_prefix(&base).ok()?.to_string_lossy().to_string();
                    let executable = !matches!(directory, "drive_c" | "home") || ["desktop", "downloads", "documents", "pictures", "music", "videos"].iter().filter_map(|name| root(name).ok()?.canonicalize().ok()).any(|root| hit.path.starts_with(root));
                    if !executable {
                        if let Ok(mut resources) = tool_state.resources.lock() {
                            if let Some(entry) = resources.get_mut(&resource_id) { entry.search_only = true; }
                        }
                    }
                    Some(json!({"resourceId":resource_id,"displayName":hit.name,"relativePath":relative_path,"directory":directory,"executable":executable,"score":hit.score}))
                })
                .collect::<Vec<Value>>();
            Ok(ok(
                tool,
                json!({"results":results,"partial":partial,"skippedDirectories":skipped,"scope":directory}),
            ))
        }
        "file.open" => {
            let o = args(arguments, &["resourceId", "directory", "path"])?;
            let path = if let Some(id) = o.get("resourceId").and_then(Value::as_str) {
                resource(tool_state, id, false)?
            } else if !o.contains_key("directory") {
                allowed_absolute_file(string(o, "path")?)?
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
            let o = args(
                arguments,
                &[
                    "parent",
                    "parentResourceId",
                    "parentDirectory",
                    "parentPath",
                    "name",
                ],
            )?;
            let path = folder_parent(tool_state, o)?.join(single_name(string(o, "name")?)?);
            if path.exists() {
                return Err("DESTINATION_EXISTS");
            }
            fs::create_dir(&path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    "PERMISSION_DENIED"
                } else {
                    "INVALID_ARGUMENT"
                }
            })?;
            Ok(ok(tool, json!({"created":true})))
        }
        "file.rename" | "file.move" | "file.copy" => {
            let o = if tool == "file.rename" {
                args(
                    arguments,
                    &[
                        "resourceId",
                        "directory",
                        "path",
                        "newName",
                        "overwriteToken",
                    ],
                )?
            } else {
                args(
                    arguments,
                    &[
                        "resourceId",
                        "sourceDirectory",
                        "sourcePath",
                        "destinationDirectory",
                        "destinationPath",
                        "overwriteToken",
                    ],
                )?
            };
            let source = if let Some(id) = o.get("resourceId").and_then(Value::as_str) {
                resource(tool_state, id, false)?
            } else if tool == "file.rename" {
                if o.contains_key("directory") {
                    existing(string(o, "directory")?, string(o, "path")?, false)?
                } else {
                    allowed_absolute_file(string(o, "path")?)?
                }
            } else {
                existing(
                    string(o, "sourceDirectory")?,
                    string(o, "sourcePath")?,
                    false,
                )?
            };
            let target = if tool == "file.rename" {
                source
                    .parent()
                    .ok_or("PATH_NOT_ALLOWED")?
                    .join(single_name(string(o, "newName")?)?)
            } else {
                transfer_destination(tool, &source, o)?
            };
            if target.exists() && target.canonicalize().map_err(|_| "PATH_NOT_ALLOWED")? != target {
                return Err("PATH_NOT_ALLOWED");
            }
            let token = o
                .get("overwriteToken")
                .map(|v| v.as_str().ok_or("INVALID_ARGUMENT"))
                .transpose()?;
            match crate::file_operations::execute(
                &app.state::<crate::file_operations::FileApprovals>(),
                tool,
                arguments,
                &source,
                &target,
                token,
            )? {
                crate::file_operations::Outcome::Conflict(conflict) => {
                    let mut result = fail(tool, "FILE_OVERWRITE_CONFIRMATION_REQUIRED");
                    result.data = Some(json!({"conflict":conflict}));
                    Ok(result)
                }
                crate::file_operations::Outcome::Done => {
                    let id = register_resource(tool_state, target.clone(), false)?;
                    Ok(ok(
                        tool,
                        json!({"completed":true,"resourceId":id,"displayName":target.file_name().and_then(|v|v.to_str())}),
                    ))
                }
            }
        }
        "file.delete" => {
            let o = args(arguments, &["resourceId", "directory", "path"])?;
            let path = if let Some(id) = o.get("resourceId").and_then(Value::as_str) {
                resource(tool_state, id, false)?
            } else {
                if !o.contains_key("directory") {
                    let path = allowed_absolute_file(string(o, "path")?)?;
                    native_windows::recycle(&path)?;
                    return Ok(ok(tool, json!({"recycled":true})));
                }
                let unresolved = root(string(o, "directory")?)?.join(relative(string(o, "path")?)?);
                if fs::symlink_metadata(unresolved)
                    .map_err(|_| "FILE_NOT_FOUND")?
                    .file_type()
                    .is_symlink()
                {
                    return Err("RECYCLE_UNAVAILABLE");
                }
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
            let text = o
                .get("text")
                .and_then(Value::as_str)
                .ok_or("INVALID_ARGUMENT")?;
            if text.len() > MAX_CLIPBOARD || text.contains('\0') {
                return Err("INVALID_ARGUMENT");
            }
            native_windows::clipboard_write(app, text)?;
            Ok(ok(tool, json!({"written":true})))
        }
        "clipboard.read" => {
            args(arguments, &[])?;
            let text = native_windows::clipboard_read()?;
            Ok(ok(tool, json!({"text":text})))
        }
        "voice.activate" => {
            args(arguments, &[])?;
            let host = cpal::default_host();
            let device = crate::audio_input::select_device(app, &host)
                .map_err(|_| "MICROPHONE_DISCONNECTED")?;
            use cpal::traits::DeviceTrait;
            device
                .default_input_config()
                .map_err(|_| "MICROPHONE_UNAVAILABLE")?;
            if !voice_overlay::request_activation(app).map_err(|_| "EXECUTION_FAILED")? {
                return Ok(ok(tool, json!({"state":"ACTIVE","alreadyActive":true})));
            }
            Ok(ok(
                tool,
                json!({"state":"ACTIVATING","activationRequested":true}),
            ))
        }
        "screen.capture" => {
            let o = args(arguments, &["monitorId", "mode"])?;
            let monitor = o
                .get("monitorId")
                .map(|v| v.as_str().ok_or("INVALID_ARGUMENT"))
                .transpose()?;
            let mode = o
                .get("mode")
                .map(|v| v.as_str().ok_or("INVALID_ARGUMENT"))
                .transpose()?;
            match crate::screen_capture::request(
                app,
                monitor,
                mode,
                capture_trace.ok_or("INVALID_ARGUMENT")?,
            )? {
                crate::screen_capture::Start::Choices(monitors) => {
                    let mut result = fail(tool, "CAPTURE_SELECTION_REQUIRED");
                    result.data = Some(json!({"monitors":monitors}));
                    Ok(result)
                }
                crate::screen_capture::Start::Pending(id) => {
                    Ok(ok(tool, json!({"captureSessionId":id})))
                }
            }
        }
        "system.open_settings" => {
            let o = args(arguments, &["page"])?;
            let page = settings_page(o)?;
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
    #[test]
    fn settings_page_accepts_only_allowlisted_strings() {
        use serde_json::json;
        for value in [json!({}), json!({"page":""}), json!({"page":"display"})] {
            assert!(super::settings_page(value.as_object().unwrap()).is_ok());
        }
        for value in [
            json!({"page":null}),
            json!({"page":5}),
            json!({"page":"ms-settings:display"}),
            json!({"page":"https://example.com"}),
            json!({"page":"cmd /c calc"}),
        ] {
            assert_eq!(
                super::settings_page(value.as_object().unwrap()),
                Err("INVALID_ARGUMENT")
            );
        }
    }
    #[test]
    fn close_auto_selection_requires_exactly_one_valid_candidate() {
        let mut result = super::fail("app.close", "WINDOW_SELECTION_REQUIRED");
        result.data = Some(serde_json::json!({"choices":[{"targetId":"bound-window"}]}));
        assert_eq!(super::single_close_target(&result), Some("bound-window"));
        result.data = Some(serde_json::json!({"choices":[]}));
        assert_eq!(super::single_close_target(&result), None);
        result.data = Some(serde_json::json!({"choices":[{"targetId":"a"},{"targetId":"b"}]}));
        assert_eq!(super::single_close_target(&result), None);
        result.data = Some(serde_json::json!({"choices":[{"targetId":null}]}));
        assert_eq!(super::single_close_target(&result), None);
    }
    #[test]
    fn exported_nest_contract_has_24_local_tools_with_rust_policies() {
        let contract: Value =
            serde_json::from_str(include_str!("../../docs/native-tool-contract.json")).unwrap();
        let tools = contract["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 24);
        let mut seen = HashSet::new();
        for tool in tools {
            let name = tool["name"].as_str().unwrap();
            assert!(seen.insert(name));
            assert_eq!(tool["executionLocation"], "LOCAL");
            assert!(
                policy(name) != Risk::Blocked,
                "missing Native policy: {name}"
            );
            assert_eq!(
                tool["systemConfirmation"].as_bool().unwrap(),
                policy(name) == Risk::Confirm
            );
        }
    }
    #[test]
    fn changed_resource_and_reserved_names_are_rejected() {
        let path =
            std::env::temp_dir().join(format!("ace-resource-change-{}.txt", uuid::Uuid::new_v4()));
        fs::write(&path, b"first").unwrap();
        let state = NativeToolState::default();
        let id = register_resource(&state, path.clone(), false).unwrap();
        fs::write(&path, b"changed contents").unwrap();
        assert_eq!(
            resource(&state, &id, false).unwrap_err(),
            "RESOURCE_NOT_FOUND"
        );
        fs::remove_file(path).unwrap();
        for name in ["CON.txt", "COM¹.txt", "a/b.txt", "../x", "name."] {
            assert!(single_name(name).is_err());
        }
        assert_eq!(single_name("a가.txt").unwrap(), "a가.txt");
    }
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
        assert!(policy("clipboard.read") == Risk::Safe);
        assert!(policy("clipboard.write") == Risk::Confirm);
    }
    #[test]
    fn traversal_and_absolute_paths_are_rejected() {
        assert_eq!(relative("../secret").unwrap_err(), "PATH_NOT_ALLOWED");
        assert_eq!(relative("C:\\secret").unwrap_err(), "PATH_NOT_ALLOWED")
    }
    #[test]
    fn path_only_file_open_keeps_user_folder_boundary() {
        assert_eq!(
            allowed_absolute_file("relative.txt").unwrap_err(),
            "INVALID_ARGUMENT"
        );
        let system_file =
            PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()))
                .join("System32\\notepad.exe");
        if system_file.is_file() {
            assert_eq!(
                allowed_absolute_file(system_file.to_str().unwrap()).unwrap_err(),
                "PATH_NOT_ALLOWED"
            );
        }
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
    #[cfg(windows)]
    fn notepad_prefers_trusted_system_launcher_over_store_alias() {
        let mut system = app("Notepad", &["notepad", "메모장"]);
        system.executable_path = std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap())
            .join("System32/notepad.exe");
        let store = app("Notepad", &["notepad", "메모장"]);
        let result = resolve_application(
            &[store, system.clone()],
            Some("notepad"),
            &["notepad".into()],
        )
        .unwrap();
        assert_eq!(result.executable_path, system.executable_path);
    }
    #[cfg(windows)]
    #[test]
    fn discovered_notepad_resolves_without_launching_an_application() {
        let registry = crate::local_commands::discover_apps();
        let expected = PathBuf::from(std::env::var_os("WINDIR").unwrap())
            .join("System32/notepad.exe")
            .canonicalize()
            .unwrap();
        for name in ["notepad", "메모장"] {
            let resolved = resolve_application(&registry, Some(name), &[name.to_owned()]).unwrap();
            assert_eq!(resolved.executable_path.canonicalize().unwrap(), expected);
        }
    }
    #[test]
    fn known_folder_candidates_resolve_in_korean_and_english() {
        assert_eq!(resolve_folder(&json!({"canonicalId":"home","original":"홈","candidates":["홈","Home","사용자 폴더"]})).unwrap(), "home");
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
    fn name_search_reaches_deep_folders_and_copy_preserves_source() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.local")
            .join(format!("search-copy-{}", uuid::Uuid::new_v4()));
        let deep = base.join("a/b/c/d/e/f/g");
        fs::create_dir_all(&deep).unwrap();
        let source = deep.join("원본.txt");
        fs::write(&source, b"isolated test content").unwrap();
        let hits = search_in(&base, &["원본.txt".into()], &[], false, 20).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].path.canonicalize().unwrap(),
            source.canonicalize().unwrap()
        );
        let source = source.canonicalize().unwrap();
        let state = NativeToolState::default();
        let id = register_resource(&state, source.clone(), false).unwrap();
        let args = json!({"resourceId":id});
        let before = approval_snapshot(&state, "file.copy", &args).unwrap();
        let target = transfer_destination("file.copy", &source, args.as_object().unwrap()).unwrap();
        assert_eq!(target.file_name().unwrap(), "원본의 복사본.txt");
        let outcome = crate::file_operations::execute(
            &crate::file_operations::FileApprovals::default(),
            "file.copy",
            &args,
            &source,
            &target,
            None,
        )
        .unwrap();
        assert!(matches!(outcome, crate::file_operations::Outcome::Done));
        assert_eq!(fs::read(&source).unwrap(), fs::read(&target).unwrap());
        assert_ne!(
            before,
            approval_snapshot(&state, "file.copy", &args).unwrap(),
            "new target must invalidate previous snapshot"
        );
        let next = transfer_destination("file.copy", &source, args.as_object().unwrap()).unwrap();
        assert_eq!(next.file_name().unwrap(), "원본의 복사본 (2).txt");
        assert!(transfer_destination("file.move", &source, args.as_object().unwrap()).is_err());
        state
            .resources
            .lock()
            .unwrap()
            .get_mut(args["resourceId"].as_str().unwrap())
            .unwrap()
            .search_only = true;
        assert_eq!(
            resource(&state, args["resourceId"].as_str().unwrap(), false).unwrap_err(),
            "PATH_NOT_ALLOWED"
        );
        fs::remove_dir_all(&base).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn drive_c_search_finds_isolated_name_without_given_path() {
        let name = format!("ace-search-fixture-{}.txt", uuid::Uuid::new_v4());
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.local")
            .join("search-drive-fixture");
        fs::create_dir_all(&base).unwrap();
        let file = base.join(&name);
        fs::write(&file, b"test only").unwrap();
        let result = search("drive_c", &[name], &[], false, 20);
        fs::remove_file(&file).unwrap();
        let (hits, partial, skipped) = result.unwrap();
        assert!(
            hits.iter()
                .any(|hit| hit.path.file_name() == file.file_name()),
            "fixture not found; partial={partial}; skipped={skipped}"
        );
        // Only fixture names/counts are inspected. No user file content is read.
    }
    #[test]
    fn explicit_search_path_does_not_choose_same_name_in_another_folder() {
        let directory =
            std::env::temp_dir().join(format!("ace-exact-search-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(directory.join("ACE 테스트")).unwrap();
        fs::write(directory.join("ACE 테스트/원본.txt"), b"intended").unwrap();
        fs::write(directory.join("원본.txt"), b"other").unwrap();
        let hits = search_in(&directory, &["ACE 테스트/원본.txt".into()], &[], false, 50).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].path,
            directory
                .join("ACE 테스트/원본.txt")
                .canonicalize()
                .unwrap()
        );
        assert!(
            search_in(&directory, &["없는 폴더/원본.txt".into()], &[], false, 50)
                .unwrap()
                .is_empty()
        );
        assert!(search_in(&directory, &["../원본.txt".into()], &[], false, 50).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn resource_id_supports_search_then_action_and_expires() {
        let path = std::env::temp_dir().join(format!("ace-resource-{}.txt", timestamp()));
        fs::write(&path, b"ace").unwrap();
        let state = NativeToolState::default();
        let id = register_resource(&state, path.clone(), false).unwrap();
        assert_eq!(
            resource(&state, &id, false).unwrap(),
            path.canonicalize().unwrap()
        );
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

    #[test]
    fn resource_log_accepts_only_generated_id_shape() {
        assert!(loggable_resource_id("file_123456789"));
        assert!(loggable_resource_id("folder_987654321"));
        assert!(!loggable_resource_id("file_123\naccess_token=secret"));
        assert!(!loggable_resource_id("C:\\Users\\name\\secret.txt"));
        assert!(!loggable_resource_id(""));
    }
}
