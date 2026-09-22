use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

const DUPLICATE_WINDOW: Duration = Duration::from_secs(2);
const REGISTRY_FILE: &str = "installed-apps.json";
const EXECUTION_LOG_FILE: &str = "native-execution.jsonl";

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Risk {
    Safe,
    Confirm,
    Blocked,
}

impl Risk {
    fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "SAFE",
            Self::Confirm => "CONFIRM",
            Self::Blocked => "BLOCKED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstalledApp {
    pub(crate) name: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) executable_path: PathBuf,
    pub(crate) process_name: String,
    pub(crate) app_type: String,
    pub(crate) last_verified: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledAppSummary {
    name: String,
    aliases: Vec<String>,
    app_type: String,
    last_verified: u64,
}

#[derive(Default)]
pub struct NativeExecutionState {
    pub(crate) registry: Mutex<Vec<InstalledApp>>,
    recent_requests: Mutex<HashMap<String, Instant>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionResult {
    success: bool,
    message: Option<String>,
    error_code: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExecutionLog<'a> {
    tool: &'a str,
    risk: &'a str,
    target_app: Option<&'a str>,
    success: bool,
    duration_ms: u128,
    error_code: Option<&'a str>,
    timestamp_ms: u128,
}

fn policy(command: &str) -> Risk {
    match command {
        "system.status" => Risk::Safe,
        "app.open" | "app.close" => Risk::Confirm,
        _ => Risk::Blocked,
    }
}

fn failed(code: &str) -> ExecutionResult {
    ExecutionResult {
        success: false,
        message: None,
        error_code: Some(code.into()),
    }
}

fn success(message: String) -> ExecutionResult {
    ExecutionResult {
        success: true,
        message: Some(message),
        error_code: None,
    }
}

fn normalize_alias(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|character| !character.is_whitespace() && !['-', '_', '.'].contains(character))
        .collect()
}

fn blocked_executable(file_name: &str) -> bool {
    matches!(
        file_name.to_ascii_lowercase().as_str(),
        "cmd.exe"
            | "powershell.exe"
            | "pwsh.exe"
            | "wscript.exe"
            | "cscript.exe"
            | "mshta.exe"
            | "rundll32.exe"
            | "reg.exe"
            | "regedit.exe"
    )
}

#[cfg(windows)]
fn allowed_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for variable in [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "LOCALAPPDATA",
        "WINDIR",
    ] {
        if let Some(value) = std::env::var_os(variable) {
            let root = PathBuf::from(value);
            roots.push(if variable == "LOCALAPPDATA" {
                root.join("Programs")
            } else if variable == "WINDIR" {
                root.join("System32")
            } else {
                root
            });
        }
    }
    roots
}

#[cfg(not(windows))]
fn allowed_roots() -> Vec<PathBuf> {
    Vec::new()
}

pub(crate) fn verified_executable(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute()
        || !path.is_file()
        || path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            != Some("exe".into())
    {
        return None;
    }
    let file_name = path.file_name()?.to_str()?;
    if blocked_executable(file_name) {
        return None;
    }
    let canonical = path.canonicalize().ok()?;
    let allowed = allowed_roots().into_iter().any(|root| {
        root.canonicalize()
            .map(|allowed| canonical.starts_with(allowed))
            .unwrap_or(false)
    });
    allowed.then_some(canonical)
}

fn app_record(
    name: &str,
    aliases: &[&str],
    path: PathBuf,
    process_name: Option<&str>,
) -> Option<InstalledApp> {
    let executable_path = verified_executable(&path)?;
    let image = process_name
        .map(str::to_owned)
        .or_else(|| executable_path.file_name()?.to_str().map(str::to_owned))?;
    Some(InstalledApp {
        name: name.into(),
        aliases: aliases.iter().map(|alias| (*alias).into()).collect(),
        executable_path,
        process_name: image,
        app_type: "desktop".into(),
        last_verified: timestamp_ms() as u64,
    })
}

#[cfg(windows)]
fn registry_app_paths() -> Vec<PathBuf> {
    use std::os::windows::process::CommandExt;
    let mut paths = Vec::new();
    for hive in [
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\App Paths",
        r"HKLM\Software\Microsoft\Windows\CurrentVersion\App Paths",
    ] {
        let output = Command::new("reg.exe")
            .args(["query", hive, "/s", "/ve"])
            .creation_flags(0x08000000)
            .output();
        let Ok(output) = output else { continue };
        if !output.status.success() {
            continue;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let Some((_, value)) = line.split_once("REG_SZ") else {
                continue;
            };
            let candidate = value.trim().trim_matches('"');
            if candidate.to_ascii_lowercase().ends_with(".exe") {
                paths.push(PathBuf::from(candidate));
            }
        }
    }
    paths
}

#[cfg(not(windows))]
fn registry_app_paths() -> Vec<PathBuf> {
    Vec::new()
}

fn discover_apps() -> Vec<InstalledApp> {
    let mut apps = Vec::new();
    #[cfg(windows)]
    {
        if let Some(windows) = std::env::var_os("WINDIR").map(PathBuf::from) {
            if let Some(app) = app_record(
                "Notepad",
                &["notepad", "메모장"],
                windows.join("System32/notepad.exe"),
                None,
            ) {
                apps.push(app);
            }
            if let Some(app) = app_record(
                "Calculator",
                &["calculator", "calc", "계산기"],
                windows.join("System32/calc.exe"),
                Some("CalculatorApp.exe"),
            ) {
                apps.push(app);
            }
        }
        let vscode_candidates = [
            std::env::var_os("LOCALAPPDATA")
                .map(|root| PathBuf::from(root).join("Programs/Microsoft VS Code/Code.exe")),
            std::env::var_os("ProgramFiles")
                .map(|root| PathBuf::from(root).join("Microsoft VS Code/Code.exe")),
        ];
        for candidate in vscode_candidates.into_iter().flatten() {
            if let Some(app) = app_record(
                "Visual Studio Code",
                &["vscode", "vs code", "code", "코드", "비주얼 스튜디오 코드"],
                candidate,
                None,
            ) {
                apps.push(app);
                break;
            }
        }
    }
    for path in registry_app_paths() {
        let Some(stem) = path
            .file_stem()
            .and_then(|value| value.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        if let Some(app) = app_record(&stem, &[&stem], path, None) {
            apps.push(app);
        }
    }
    let mut unique = HashSet::new();
    apps.retain(|app| unique.insert(app.executable_path.to_string_lossy().to_ascii_lowercase()));
    apps
}

fn cache_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|path| path.join(REGISTRY_FILE))
}

pub fn initialize(app: &tauri::AppHandle) -> NativeExecutionState {
    let cached: Vec<InstalledApp> = cache_path(app)
        .and_then(|path| fs::read(path).ok())
        .and_then(|data| serde_json::from_slice::<Vec<InstalledApp>>(&data).ok())
        .unwrap_or_default();
    // The cache is never an authority for executable paths. Only entries rediscovered from
    // trusted Windows sources are admitted; cached aliases are retained for matching paths.
    let mut registry = discover_apps();
    for app in &mut registry {
        if let Some(previous) = cached
            .iter()
            .find(|item| item.executable_path == app.executable_path)
        {
            for alias in &previous.aliases {
                if !app
                    .aliases
                    .iter()
                    .any(|value| normalize_alias(value) == normalize_alias(alias))
                {
                    app.aliases.push(alias.clone());
                }
            }
        }
    }
    if let Some(path) = cache_path(app) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_vec_pretty(&registry) {
            let _ = fs::write(path, data);
        }
    }
    NativeExecutionState {
        registry: Mutex::new(registry),
        recent_requests: Mutex::new(HashMap::new()),
    }
}

pub(crate) fn resolve_app(
    registry: &[InstalledApp],
    alias: &str,
) -> Result<InstalledApp, &'static str> {
    let requested = normalize_alias(alias);
    if requested.is_empty() || requested.len() > 80 {
        return Err("INVALID_ARGUMENTS");
    }
    let matches: Vec<_> = registry
        .iter()
        .filter(|app| {
            normalize_alias(&app.name) == requested
                || app
                    .aliases
                    .iter()
                    .any(|candidate| normalize_alias(candidate) == requested)
        })
        .collect();
    if matches.len() != 1 {
        return Err(if matches.is_empty() {
            "APP_NOT_FOUND"
        } else {
            "AMBIGUOUS_APP"
        });
    }
    let mut target = matches[0].clone();
    target.executable_path =
        verified_executable(&target.executable_path).ok_or("STALE_APP_PATH")?;
    Ok(target)
}

fn validate_app_request(arguments: &Value) -> Result<&str, &'static str> {
    let object = arguments.as_object().ok_or("INVALID_ARGUMENTS")?;
    if object.len() != 1 {
        return Err("INVALID_ARGUMENTS");
    }
    object
        .get("appName")
        .and_then(Value::as_str)
        .ok_or("INVALID_ARGUMENTS")
}

fn duplicate(state: &NativeExecutionState, command: &str, app: &InstalledApp) -> bool {
    let key = format!(
        "{}:{}",
        command,
        app.executable_path.to_string_lossy().to_ascii_lowercase()
    );
    let Ok(mut recent) = state.recent_requests.lock() else {
        return true;
    };
    let now = Instant::now();
    recent.retain(|_, instant| now.duration_since(*instant) < DUPLICATE_WINDOW);
    if let std::collections::hash_map::Entry::Vacant(entry) = recent.entry(key) {
        entry.insert(now);
        false
    } else {
        true
    }
}

fn timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn write_log(app: &tauri::AppHandle, entry: &ExecutionLog<'_>) {
    let Ok(directory) = app.path().app_log_dir() else {
        return;
    };
    if fs::create_dir_all(&directory).is_err() {
        return;
    }
    let Ok(line) = serde_json::to_string(entry) else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join(EXECUTION_LOG_FILE))
    {
        let _ = writeln!(file, "{line}");
    }
}

#[tauri::command]
pub fn list_installed_apps(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, NativeExecutionState>,
) -> Result<Vec<InstalledAppSummary>, String> {
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    let registry = state
        .registry
        .lock()
        .map_err(|_| "REGISTRY_UNAVAILABLE".to_owned())?;
    Ok(registry
        .iter()
        .map(|app| InstalledAppSummary {
            name: app.name.clone(),
            aliases: app.aliases.clone(),
            app_type: app.app_type.clone(),
            last_verified: app.last_verified,
        })
        .collect())
}

#[tauri::command]
pub fn execute_local_command(
    app_handle: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, NativeExecutionState>,
    command_type: String,
    arguments: Value,
    confirmed: bool,
) -> ExecutionResult {
    let started = Instant::now();
    let risk = policy(&command_type);
    let mut target_name: Option<String> = None;
    let result = if window.label() != "main" || risk == Risk::Blocked {
        failed("BLOCKED")
    } else if risk == Risk::Confirm && !confirmed {
        failed("CONFIRMATION_REQUIRED")
    } else if command_type == "system.status" {
        if arguments
            .as_object()
            .is_some_and(|object| object.is_empty())
        {
            success(format!(
                "시스템: {} / {} · CPU 스레드: {}",
                std::env::consts::OS,
                std::env::consts::ARCH,
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1)
            ))
        } else {
            failed("INVALID_ARGUMENTS")
        }
    } else {
        let requested = match validate_app_request(&arguments) {
            Ok(value) => value,
            Err(code) => {
                return logged_result(
                    &app_handle,
                    &command_type,
                    risk,
                    None,
                    started,
                    failed(code),
                )
            }
        };
        let target = {
            let registry = match state.registry.lock() {
                Ok(value) => value,
                Err(_) => {
                    return logged_result(
                        &app_handle,
                        &command_type,
                        risk,
                        None,
                        started,
                        failed("REGISTRY_UNAVAILABLE"),
                    )
                }
            };
            match resolve_app(&registry, requested) {
                Ok(value) => value,
                Err(code) => {
                    return logged_result(
                        &app_handle,
                        &command_type,
                        risk,
                        None,
                        started,
                        failed(code),
                    )
                }
            }
        };
        target_name = Some(target.name.clone());
        if duplicate(&state, &command_type, &target) {
            failed("DUPLICATE_REQUEST")
        } else {
            execute_app(&command_type, &target)
        }
    };
    logged_result(
        &app_handle,
        &command_type,
        risk,
        target_name.as_deref(),
        started,
        result,
    )
}

fn logged_result(
    app: &tauri::AppHandle,
    tool: &str,
    risk: Risk,
    target: Option<&str>,
    started: Instant,
    result: ExecutionResult,
) -> ExecutionResult {
    write_log(
        app,
        &ExecutionLog {
            tool,
            risk: risk.as_str(),
            target_app: target,
            success: result.success,
            duration_ms: started.elapsed().as_millis(),
            error_code: result.error_code.as_deref(),
            timestamp_ms: timestamp_ms(),
        },
    );
    result
}

#[cfg(windows)]
pub(crate) fn execute_app(command: &str, target: &InstalledApp) -> ExecutionResult {
    use std::os::windows::process::CommandExt;
    let output = if command == "app.open" {
        Command::new(&target.executable_path).spawn().map(|_| ())
    } else if command == "app.close" {
        Command::new("taskkill.exe")
            .args(["/IM", &target.process_name, "/T"])
            .creation_flags(0x08000000)
            .output()
            .and_then(|output| {
                if output.status.success() {
                    Ok(())
                } else {
                    Err(std::io::Error::other("process not running"))
                }
            })
    } else {
        return failed("BLOCKED");
    };
    match output {
        Ok(()) => success(format!(
            "{} {} 요청 완료",
            target.name,
            if command == "app.open" {
                "실행"
            } else {
                "종료"
            }
        )),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            failed("ELEVATION_REQUIRED")
        }
        Err(_) => failed("EXECUTION_FAILED"),
    }
}

#[cfg(not(windows))]
fn execute_app(_: &str, _: &InstalledApp) -> ExecutionResult {
    failed("BLOCKED")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fake(name: &str, aliases: &[&str], path: &str) -> InstalledApp {
        InstalledApp {
            name: name.into(),
            aliases: aliases.iter().map(|value| (*value).into()).collect(),
            executable_path: PathBuf::from(path),
            process_name: format!("{name}.exe"),
            app_type: "desktop".into(),
            last_verified: 0,
        }
    }

    #[test]
    fn shell_and_unknown_commands_remain_blocked() {
        for command in [
            "shell.exec",
            "powershell.exec",
            "cmd.exec",
            "registry.write",
            "unknown",
        ] {
            assert_eq!(policy(command), Risk::Blocked);
        }
    }

    #[test]
    fn raw_paths_and_extra_arguments_are_rejected() {
        assert_eq!(
            validate_app_request(&json!({"appName":"Code", "path":"C:\\evil.exe"})),
            Err("INVALID_ARGUMENTS")
        );
        assert_eq!(
            validate_app_request(&json!({"executablePath":"C:\\evil.exe"})),
            Err("INVALID_ARGUMENTS")
        );
    }

    #[test]
    fn aliases_are_exact_and_ambiguous_matches_are_blocked_before_execution() {
        let registry = vec![fake(
            "Visual Studio Code",
            &["vscode", "코드"],
            "C:/Code.exe",
        )];
        assert_eq!(normalize_alias("VS Code"), normalize_alias("vs-code"));
        assert_eq!(
            resolve_app(&registry, "not installed"),
            Err("APP_NOT_FOUND")
        );
        assert_eq!(resolve_app(&registry, "vscode"), Err("STALE_APP_PATH"));
        let duplicate = vec![
            fake("One", &["code"], "C:/One.exe"),
            fake("Two", &["code"], "C:/Two.exe"),
        ];
        assert_eq!(resolve_app(&duplicate, "code"), Err("AMBIGUOUS_APP"));
    }

    #[test]
    fn dangerous_shell_hosts_are_never_registry_targets() {
        for executable in [
            "cmd.exe",
            "powershell.exe",
            "pwsh.exe",
            "wscript.exe",
            "mshta.exe",
        ] {
            assert!(blocked_executable(executable));
        }
        assert!(!blocked_executable("Code.exe"));
    }

    #[test]
    fn confirmation_and_duplicate_request_policy_are_enforced() {
        assert_eq!(policy("app.open"), Risk::Confirm);
        let state = NativeExecutionState::default();
        let app = fake("Code", &["code"], "C:/Code.exe");
        assert!(!duplicate(&state, "app.open", &app));
        assert!(duplicate(&state, "app.open", &app));
    }

    #[cfg(windows)]
    #[test]
    fn windows_discovery_contains_verified_system_apps() {
        let apps = discover_apps();
        assert!(apps.iter().any(|app| app.name == "Notepad"));
        assert!(apps.iter().any(|app| app.name == "Calculator"));
        let vscode_installed = std::env::var_os("LOCALAPPDATA")
            .map(|root| PathBuf::from(root).join("Programs/Microsoft VS Code/Code.exe"))
            .is_some_and(|path| path.is_file());
        if vscode_installed {
            let vscode = apps
                .iter()
                .find(|app| app.name == "Visual Studio Code")
                .expect("installed VS Code must be discovered");
            assert!(vscode
                .aliases
                .iter()
                .any(|alias| normalize_alias(alias) == normalize_alias("vs code")));
        }
        assert!(apps
            .iter()
            .all(|app| verified_executable(&app.executable_path).is_some()));
    }
}
