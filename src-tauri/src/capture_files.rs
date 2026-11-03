use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
const TTL: u64 = 3600;
#[derive(Default)]
pub struct CaptureFiles(pub Mutex<HashMap<String, PathBuf>>);
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    task_id: String,
    created_at: u64,
    expires_at: u64,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_cache_dir()
        .map_err(|_| "CAPTURE_STORAGE_FAILED")?
        .join("capture-temp"))
}
pub fn record(path: &Path, task_id: &str) -> Result<(), String> {
    let value = Metadata {
        task_id: task_id.into(),
        created_at: now(),
        expires_at: now() + TTL,
    };
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path.with_extension("json"))
        .map_err(|_| "CAPTURE_STORAGE_FAILED")?;
    serde_json::to_writer(&file, &value).map_err(|_| "CAPTURE_STORAGE_FAILED")?;
    file.sync_all().map_err(|_| "CAPTURE_STORAGE_FAILED".into())
}
pub fn pin(app: &tauri::AppHandle, id: String, path: PathBuf) -> Result<(), String> {
    app.state::<CaptureFiles>()
        .0
        .lock()
        .map_err(|_| "CAPTURE_STORAGE_FAILED")?
        .insert(id, path);
    Ok(())
}
fn remove(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let _ = fs::remove_file(path.with_extension("json"));
    Ok(())
}
fn clean(dir: &Path, active: &HashMap<String, PathBuf>, timestamp: u64) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path
            .file_stem()
            .and_then(|v| v.to_str())
            .and_then(|v| v.strip_prefix("capture-"))
        else {
            continue;
        };
        if uuid::Uuid::parse_str(name).is_err()
            || !matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("png" | "json")
            )
        {
            continue;
        }
        let image = path.with_extension("png");
        if active.values().any(|p| p == &image) {
            continue;
        }
        let Ok(info) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !info.is_file() || info.file_type().is_symlink() {
            continue;
        }
        let expired = fs::read(path.with_extension("json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Metadata>(&bytes).ok())
            .map(|m| m.expires_at <= timestamp)
            .unwrap_or_else(|| {
                info.modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .is_some_and(|t| timestamp.saturating_sub(t.as_secs()) >= TTL)
            });
        if expired {
            let _ = remove(&image);
        }
    }
}
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            if let Ok(dir) = directory(&app) {
                if let Ok(active) = app.state::<CaptureFiles>().0.lock() {
                    clean(&dir, &active, now());
                }
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}
#[tauri::command]
pub fn release_capture_file(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    resource_id: String,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    if let Some(path) = app
        .state::<CaptureFiles>()
        .0
        .lock()
        .map_err(|_| "CAPTURE_STORAGE_FAILED")?
        .remove(&resource_id)
    {
        remove(&path).map_err(|_| "CAPTURE_STORAGE_FAILED")?;
    }
    Ok(())
}
#[tauri::command]
pub fn save_capture_file(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    resource_id: String,
    name: String,
) -> Result<String, String> {
    let _activity = crate::activity::begin().map_err(str::to_owned)?;
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    // Save a new copy outside the managed cache; never overwrite an existing file.
    if !name.to_ascii_lowercase().ends_with(".png")
        || name.len() > 200
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || name.starts_with('.')
    {
        return Err("PNG 파일 이름을 입력해 주세요.".into());
    }
    let state = app.state::<CaptureFiles>();
    let active = state.0.lock().map_err(|_| "CAPTURE_STORAGE_FAILED")?;
    let source = active.get(&resource_id).ok_or("캡처가 만료되었습니다.")?;
    let dir = dirs::picture_dir()
        .ok_or("사진 폴더를 찾을 수 없습니다.")?
        .join("ACE Captures");
    fs::create_dir_all(&dir).map_err(|_| "저장 폴더를 만들지 못했습니다.")?;
    let target = dir.join(name);
    let mut input = fs::File::open(source).map_err(|_| "캡처 파일을 읽지 못했습니다.")?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|_| "같은 이름의 파일이 있거나 저장할 수 없습니다.")?;
    if std::io::copy(&mut input, &mut output)
        .and_then(|_| output.sync_all())
        .is_err()
    {
        drop(output);
        let _ = fs::remove_file(&target);
        return Err("이미지를 저장하지 못했습니다.".into());
    }
    Ok(target.to_string_lossy().into_owned())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expiry_boundary_active_lease_and_saved_png_bytes() {
        let root =
            std::env::temp_dir().join(format!("ace-capture-lifecycle-{}", uuid::Uuid::new_v4()));
        let cache = root.join("capture-temp");
        let saved_dir = root.join("user-saved");
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir(&saved_dir).unwrap();
        let create = |expires_at: u64| {
            let path = cache.join(format!("capture-{}.png", uuid::Uuid::new_v4()));
            let file = fs::File::create(&path).unwrap();
            let mut encoder = png::Encoder::new(file, 2, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[42; 16])
                .unwrap();
            fs::write(
                path.with_extension("json"),
                serde_json::to_vec(&Metadata {
                    task_id: uuid::Uuid::new_v4().to_string(),
                    created_at: 100,
                    expires_at,
                })
                .unwrap(),
            )
            .unwrap();
            path
        };
        let expired = create(200);
        let future = create(201);
        let leased = create(100);
        let saved = saved_dir.join("capture-copy.png");
        fs::copy(&expired, &saved).unwrap();
        let original = fs::read(&expired).unwrap();
        let active = HashMap::from([("preview".into(), leased.clone())]);
        clean(&cache, &active, 200);
        assert!(!expired.exists());
        assert!(!expired.with_extension("json").exists());
        assert!(future.exists());
        assert!(leased.exists());
        assert_eq!(fs::read(&saved).unwrap(), original);
        // Model a restarted process's empty lease map; this is not an actual app restart.
        clean(&cache, &HashMap::new(), 200);
        assert!(!leased.exists());
        assert!(future.exists());
        clean(&cache, &HashMap::new(), 201);
        assert!(!future.exists());
        assert_eq!(fs::read(&saved).unwrap(), original);
        fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn restart_cleanup_preserves_leased_and_user_files() {
        let dir = std::env::temp_dir().join(format!("ace-cache-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join(format!("capture-{}.png", uuid::Uuid::new_v4()));
        fs::write(&path, b"image").unwrap();
        record(&path, "test-task").unwrap();
        let saved = dir.join("user-saved.png");
        fs::write(&saved, b"saved").unwrap();
        let mut active = HashMap::new();
        active.insert("preview".into(), path.clone());
        clean(&dir, &active, now() + TTL + 1);
        assert!(path.exists());
        clean(&dir, &HashMap::new(), now() + TTL + 1);
        assert!(!path.exists());
        assert!(saved.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
