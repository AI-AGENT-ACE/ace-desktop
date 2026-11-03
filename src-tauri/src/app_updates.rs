use serde::Serialize;
use std::sync::Mutex;
use tauri::Manager;
use tauri_plugin_updater::{Update, UpdaterExt};
const ENDPOINT: &str =
    "https://github.com/AI-AGENT-ACE/ace-desktop/releases/latest/download/latest.json";
#[cfg(not(debug_assertions))]
pub fn register(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let config = app
        .config()
        .plugins
        .0
        .get("updater")
        .ok_or("Release updater configuration is missing")?;
    let key = option_env!("ACE_UPDATER_PUBLIC_KEY").unwrap_or("").trim();
    if key.is_empty()
        || config.get("pubkey").and_then(|v| v.as_str()) != Some(key)
        || config.get("endpoints") != Some(&serde_json::json!([ENDPOINT]))
    {
        return Err("Release updater public key or endpoint is missing or inconsistent".into());
    }
    app.plugin(tauri_plugin_updater::Builder::new().build())?;
    Ok(())
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    phase: String,
    version: Option<String>,
    notes: Option<String>,
    downloaded: u64,
    total: Option<u64>,
    message: String,
}
#[derive(Default)]
pub struct Updates {
    status: Mutex<Status>,
    update: Mutex<Option<Update>>,
    operation: Mutex<()>,
}
fn set(app: &tauri::AppHandle, phase: &str, message: &str) {
    if let Ok(mut s) = app.state::<Updates>().status.lock() {
        s.phase = phase.into();
        s.message = message.into();
    }
}
fn main(window: &tauri::WebviewWindow) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("UPDATE_UNAVAILABLE_IN_DEVELOPMENT".into());
    }
    if window.label() == "main" {
        Ok(())
    } else {
        Err("BLOCKED".into())
    }
}
#[tauri::command]
pub fn update_status(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Status, String> {
    main(&window)?;
    app.state::<Updates>()
        .status
        .lock()
        .map(|s| s.clone())
        .map_err(|_| "UPDATE_BUSY".into())
}
#[tauri::command]
pub async fn check_app_update(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    main(&window)?;
    let state = app.state::<Updates>();
    {
        let mut status = state.status.lock().map_err(|_| "UPDATE_BUSY")?;
        if ["checking", "downloading", "waiting", "installing"].contains(&status.phase.as_str()) {
            return Err("업데이트를 처리 중입니다.".into());
        }
        status.phase = "checking".into();
        status.message = "업데이트 확인 중…".into();
    }
    let key = option_env!("ACE_UPDATER_PUBLIC_KEY").unwrap_or("");
    if key.is_empty() {
        set(
            &app,
            "unconfigured",
            "업데이트 서명 공개키가 아직 설정되지 않은 빌드입니다.",
        );
        return Ok(());
    }
    let result = async {
        app.updater_builder()
            .pubkey(key)
            .endpoints(vec![ENDPOINT.parse().unwrap()])?
            .build()?
            .check()
            .await
    }
    .await;
    match result {
        Ok(Some(update)) => {
            *state.status.lock().map_err(|_| "UPDATE_BUSY")? = Status {
                phase: "available".into(),
                version: Some(update.version.clone()),
                notes: update.body.clone(),
                message: "새 버전을 설치할 수 있습니다.".into(),
                ..Default::default()
            };
            *state.update.lock().map_err(|_| "UPDATE_BUSY")? = Some(update);
        }
        Ok(None) => set(&app, "latest", "최신 버전입니다."),
        Err(_) => set(
            &app,
            "error",
            "업데이트를 확인하지 못했습니다. 현재 버전은 계속 사용할 수 있습니다.",
        ),
    }
    Ok(())
}
#[tauri::command]
pub fn install_app_update(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    main(&window)?;
    let update = {
        let state = app.state::<Updates>();
        let _operation = state.operation.lock().map_err(|_| "UPDATE_BUSY")?;
        let mut status = state.status.lock().map_err(|_| "UPDATE_BUSY")?;
        if status.phase != "available" {
            return Err("먼저 업데이트를 확인해 주세요.".into());
        }
        let update = state
            .update
            .lock()
            .map_err(|_| "UPDATE_BUSY")?
            .take()
            .ok_or("UPDATE_MISSING")?;
        status.phase = "downloading".into();
        status.downloaded = 0;
        status.message = "업데이트 다운로드 및 서명 검증 중…".into();
        update
    };
    tauri::async_runtime::spawn(async move {
        let result = update
            .download(
                |size, total| {
                    if let Ok(mut s) = app.state::<Updates>().status.lock() {
                        s.downloaded += size as u64;
                        s.total = total;
                    }
                },
                || {},
            )
            .await;
        let bytes = match result {
            Ok(bytes) => bytes,
            Err(_) => {
                set(
                    &app,
                    "error",
                    "다운로드 또는 서명 검증에 실패했습니다. 현재 버전을 유지합니다.",
                );
                return;
            }
        };
        set(
            &app,
            "waiting",
            "녹음·파일 작업이 끝나면 설치합니다. 설치 시 앱이 종료됩니다.",
        );
        loop {
            if !crate::voice_overlay::is_busy(&app) && crate::activity::try_install() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        set(&app, "installing", "업데이트 설치 중…");
        let result = update.install(bytes);
        crate::activity::finish_install();
        if result.is_err() {
            set(
                &app,
                "error",
                "설치를 시작하지 못했습니다. 현재 버전을 계속 사용할 수 있습니다.",
            );
        }
    });
    Ok(())
}
