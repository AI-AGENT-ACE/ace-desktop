use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trace {
    pub execution_id: String,
    pub attempt_id: String,
}
impl Trace {
    fn log(&self, app: &tauri::AppHandle, session: &str, stage: &str) {
        #[cfg(debug_assertions)]
        {
            use std::io::Write;
            let line = serde_json::json!({"executionId":self.execution_id,"attemptId":self.attempt_id,"sessionId":session,"stage":stage,"timestampMs":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()});
            eprintln!("[ACE capture] {line}");
            if let Ok(dir) = app.path().app_log_dir() {
                let _ = fs::create_dir_all(&dir);
                if let Ok(mut file) = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(dir.join("capture-stages.jsonl"))
                {
                    let _ = writeln!(file, "{line}");
                }
            }
        }
        #[cfg(not(debug_assertions))]
        let _ = (app, session, stage);
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Display {
    name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
}
impl Display {
    fn from(m: &tauri::Monitor) -> Self {
        Self {
            name: m.name().cloned().unwrap_or_else(|| "모니터".into()),
            x: m.position().x,
            y: m.position().y,
            width: m.size().width,
            height: m.size().height,
            scale: m.scale_factor(),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorChoice {
    monitor_id: String,
    name: String,
    width: u32,
    height: u32,
    scale_factor: f64,
}
struct Session {
    trace: Trace,
    _activity: crate::activity::Activity,
    id: String,
    display: Display,
    mode: String,
    started: Instant,
    capturing: bool,
}
#[derive(Default)]
pub struct CaptureState {
    choices: Mutex<HashMap<String, (Display, Instant)>>,
    session: Mutex<Option<Session>>,
    results: Mutex<HashMap<String, CaptureOutcome>>,
}
type CaptureOutcome = (Result<Image, String>, Instant);
pub enum Start {
    Choices(Vec<MonitorChoice>),
    Pending(String),
}
pub fn request(
    app: &tauri::AppHandle,
    monitor_id: Option<&str>,
    mode: Option<&str>,
    trace: Trace,
) -> Result<Start, &'static str> {
    trace.log(app, "", "request_validated");
    let main = app
        .get_webview_window("main")
        .ok_or("SCREEN_CAPTURE_FAILED")?;
    let available = main
        .available_monitors()
        .map_err(|_| "SCREEN_CAPTURE_FAILED")?;
    let state = app.state::<CaptureState>();
    if monitor_id.is_none() {
        if mode.is_some_and(|value| !["full", "region"].contains(&value)) {
            return Err("INVALID_ARGUMENT");
        }
        let mut choices = state.choices.lock().map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        choices.retain(|_, (_, t)| t.elapsed() < Duration::from_secs(120));
        if choices.len() > 100 {
            return Err("SCREEN_CAPTURE_FAILED");
        }
        let mut result = Vec::new();
        for m in available {
            let d = Display::from(&m);
            let id = uuid::Uuid::new_v4().to_string();
            result.push(MonitorChoice {
                monitor_id: id.clone(),
                name: d.name.clone(),
                width: d.width,
                height: d.height,
                scale_factor: d.scale,
            });
            choices.insert(id, (d, Instant::now()));
        }
        if result.is_empty() {
            return Err("CAPTURE_MONITOR_DISCONNECTED");
        }
        if mode == Some("full") && result.len() == 1 {
            let id = result[0].monitor_id.clone();
            drop(choices);
            return request(app, Some(&id), Some("full"), trace);
        }
        return Ok(Start::Choices(result));
    }
    let mode = mode
        .filter(|m| ["full", "region"].contains(m))
        .ok_or("INVALID_ARGUMENT")?;
    let (display, issued) = state
        .choices
        .lock()
        .map_err(|_| "SCREEN_CAPTURE_FAILED")?
        .remove(monitor_id.unwrap())
        .ok_or("CAPTURE_MONITOR_DISCONNECTED")?;
    if issued.elapsed() > Duration::from_secs(120)
        || !available.iter().any(|m| Display::from(m) == display)
    {
        return Err("CAPTURE_MONITOR_DISCONNECTED");
    }
    let mut session = state.session.lock().map_err(|_| "SCREEN_CAPTURE_FAILED")?;
    if session.is_some() {
        return Err("CAPTURE_BUSY");
    }
    let id = uuid::Uuid::new_v4().to_string();
    *session = Some(Session {
        trace: trace.clone(),
        _activity: crate::activity::begin()?,
        id: id.clone(),
        display: display.clone(),
        mode: mode.into(),
        started: Instant::now(),
        capturing: false,
    });
    drop(session);
    trace.log(app, &id, "selector_creating");
    let selector = WebviewWindowBuilder::new(
        app,
        "capture-selector",
        WebviewUrl::App(format!("index.html#capture?session={id}").into()),
    )
    .title("ACE 화면 캡처")
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(false)
    .build()
    .map_err(|_| {
        finish_error(app, &id, "SCREEN_CAPTURE_FAILED");
        "SCREEN_CAPTURE_FAILED"
    })?;
    let configured = (|| -> Result<(), &'static str> {
        if !connected(app, &display) {
            return Err("CAPTURE_MONITOR_DISCONNECTED");
        }
        selector
            .set_position(tauri::PhysicalPosition::new(display.x, display.y))
            .map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        selector
            .set_size(tauri::PhysicalSize::new(display.width, display.height))
            .map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        #[cfg(windows)]
        {
            use windows::Win32::{
                Foundation::HWND,
                UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE},
            };
            unsafe {
                SetWindowDisplayAffinity(
                    HWND(selector.hwnd().map_err(|_| "SCREEN_CAPTURE_FAILED")?.0),
                    WDA_EXCLUDEFROMCAPTURE,
                )
            }
            .map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        }
        selector.show().map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        selector.set_focus().map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        Ok(())
    })();
    if let Err(e) = configured {
        let _ = selector.destroy();
        finish_error(app, &id, e);
        return Err(e);
    }
    trace.log(app, &id, "selector_ready");
    let watching = app.clone();
    let watch_id = id.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let stale = {
                let s = watching.state::<CaptureState>();
                let lock = s.session.lock().unwrap_or_else(|e| e.into_inner());
                let Some(current) = lock.as_ref().filter(|j| j.id == watch_id) else {
                    return;
                };
                if current.started.elapsed() > Duration::from_secs(180) {
                    Some("CAPTURE_CANCELLED")
                } else if !connected(&watching, &current.display) {
                    Some("CAPTURE_MONITOR_DISCONNECTED")
                } else {
                    None
                }
            };
            if let Some(reason) = stale {
                finish_error(&watching, &watch_id, reason);
                return;
            }
        }
    });
    Ok(Start::Pending(id))
}
fn connected(app: &tauri::AppHandle, display: &Display) -> bool {
    app.get_webview_window("main")
        .and_then(|w| w.available_monitors().ok())
        .is_some_and(|list| list.iter().any(|m| Display::from(m) == *display))
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    session_id: String,
    name: String,
    mode: String,
    width: u32,
    height: u32,
    scale_factor: f64,
}
#[tauri::command]
pub fn get_capture_context(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Context, String> {
    if window.label() != "capture-selector" {
        return Err("BLOCKED".into());
    }
    let state = app.state::<CaptureState>();
    let session = state.session.lock().map_err(|_| "CAPTURE_BUSY")?;
    let j = session.as_ref().ok_or("CAPTURE_CANCELLED")?;
    Ok(Context {
        session_id: j.id.clone(),
        name: j.display.name.clone(),
        mode: j.mode.clone(),
        width: j.display.width,
        height: j.display.height,
        scale_factor: j.display.scale,
    })
}
#[derive(Clone, Copy, Deserialize)]
pub struct Region {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
fn physical_region(
    d: &Display,
    region: Option<Region>,
) -> Result<(i32, i32, u32, u32), &'static str> {
    let Some(r) = region else {
        return Ok((d.x, d.y, d.width, d.height));
    };
    if [r.x, r.y, r.width, r.height, d.scale]
        .iter()
        .any(|n| !n.is_finite())
        || d.scale <= 0.0
        || r.x < 0.0
        || r.y < 0.0
        || r.width <= 0.0
        || r.height <= 0.0
    {
        return Err("INVALID_ARGUMENT");
    }
    let left = (r.x * d.scale).round();
    let top = (r.y * d.scale).round();
    let right = ((r.x + r.width) * d.scale).round();
    let bottom = ((r.y + r.height) * d.scale).round();
    if right > d.width as f64
        || bottom > d.height as f64
        || right - left < 1.0
        || bottom - top < 1.0
    {
        return Err("INVALID_ARGUMENT");
    }
    Ok((
        d.x.checked_add(left as i32).ok_or("INVALID_ARGUMENT")?,
        d.y.checked_add(top as i32).ok_or("INVALID_ARGUMENT")?,
        (right - left) as u32,
        (bottom - top) as u32,
    ))
}
fn finish_error(app: &tauri::AppHandle, id: &str, reason: &str) {
    let state = app.state::<CaptureState>();
    let mut session = state.session.lock().unwrap_or_else(|e| e.into_inner());
    if session.as_ref().is_some_and(|s| s.id == id) {
        session.as_ref().unwrap().trace.log(app, id, reason);
        *session = None;
        state
            .results
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.into(), (Err(reason.into()), Instant::now()));
        drop(session);
        if let Some(w) = app.get_webview_window("capture-selector") {
            let _ = w.destroy();
        }
    } else {
        let mut results = state.results.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((result, _)) = results.get_mut(id) {
            *result = Err(reason.into());
        }
    }
}
#[tauri::command]
pub async fn cancel_screen_capture(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    session_id: String,
) -> Result<(), String> {
    if !["main", "capture-selector"].contains(&window.label()) {
        return Err("BLOCKED".into());
    }
    finish_error(&app, &session_id, "CAPTURE_CANCELLED");
    Ok(())
}
pub fn cancel_active(app: &tauri::AppHandle) {
    let id = app
        .state::<CaptureState>()
        .session
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|s| s.id.clone());
    if let Some(id) = id {
        finish_error(app, &id, "CAPTURE_CANCELLED");
    }
}
struct Image {
    trace: Trace,
    path: PathBuf,
    file: Option<fs::File>,
    keep: bool,
}
impl Image {
    fn create(path: PathBuf, trace: Trace) -> Result<Self, String> {
        let mut options = fs::OpenOptions::new();
        options.create_new(true).read(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.access_mode(0xc0010000).share_mode(1);
        }
        let file = options.open(&path).map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        Ok(Self {
            trace,
            path,
            file: Some(file),
            keep: false,
        })
    }
}
impl Drop for Image {
    fn drop(&mut self) {
        if !self.keep {
            #[cfg(windows)]
            if let Some(file) = self.file.as_ref() {
                use std::os::windows::io::AsRawHandle;
                use windows::Win32::{
                    Foundation::HANDLE,
                    Storage::FileSystem::{
                        FileDispositionInfo, SetFileInformationByHandle, FILE_DISPOSITION_INFO,
                    },
                };
                let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
                if unsafe {
                    SetFileInformationByHandle(
                        HANDLE(file.as_raw_handle()),
                        FileDispositionInfo,
                        &disposition as *const _ as _,
                        std::mem::size_of_val(&disposition) as u32,
                    )
                }
                .is_err()
                {
                    eprintln!("ACE capture temporary file cleanup failed");
                }
            }
            drop(self.file.take());
            #[cfg(not(windows))]
            let _ = fs::remove_file(&self.path);
            let _ = fs::remove_file(self.path.with_extension("json"));
        }
    }
}
#[tauri::command]
pub async fn complete_screen_capture(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    session_id: String,
    region: Option<Region>,
) -> Result<(), String> {
    if window.label() != "capture-selector" {
        return Err("BLOCKED".into());
    }
    let state = app.state::<CaptureState>();
    let (display, trace) = {
        let mut s = state.session.lock().map_err(|_| "CAPTURE_BUSY")?;
        let j = s
            .as_mut()
            .filter(|j| j.id == session_id && !j.capturing)
            .ok_or("CAPTURE_CANCELLED")?;
        if (j.mode == "region") != region.is_some() {
            return Err("INVALID_ARGUMENT".into());
        }
        j.capturing = true;
        (j.display.clone(), j.trace.clone())
    };
    trace.log(&app, &session_id, "capture_started");
    let operation = (|| -> Result<(), String> {
        if !connected(&app, &display) {
            return Err("CAPTURE_MONITOR_DISCONNECTED".into());
        }
        let rect = physical_region(&display, region).map_err(str::to_owned)?;
        window.hide().map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        #[cfg(windows)]
        unsafe {
            use windows::Win32::{
                Foundation::HWND,
                UI::WindowsAndMessaging::{IsWindowVisible, ShowWindow, SW_HIDE},
            };
            let hwnd = HWND(window.hwnd().map_err(|_| "SCREEN_CAPTURE_FAILED")?.0);
            let _ = ShowWindow(hwnd, SW_HIDE);
            if IsWindowVisible(hwnd).as_bool() {
                return Err("SCREEN_CAPTURE_FAILED".into());
            }
            windows::Win32::Graphics::Dwm::DwmFlush().map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        }
        let dir = app
            .path()
            .app_cache_dir()
            .map_err(|_| "SCREEN_CAPTURE_FAILED")?
            .join("capture-temp");
        fs::create_dir_all(&dir).map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        let path = dir.join(format!("capture-{}.png", uuid::Uuid::new_v4()));
        let mut image = Image::create(path.clone(), trace.clone())?;
        crate::capture_files::record(&path, &session_id)?;
        crate::native_windows::capture_rect_to_file(
            image.file.as_mut().unwrap(),
            rect.0,
            rect.1,
            rect.2,
            rect.3,
        )
        .map_err(str::to_owned)?;
        trace.log(&app, &session_id, "image_generated");
        if !connected(&app, &display) {
            return Err("CAPTURE_MONITOR_DISCONNECTED".into());
        }
        let s = state.session.lock().map_err(|_| "CAPTURE_BUSY")?;
        if !s.as_ref().is_some_and(|j| j.id == session_id) {
            return Err("CAPTURE_CANCELLED".into());
        }
        drop(s);
        window.destroy().map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        let mut s = state.session.lock().map_err(|_| "CAPTURE_BUSY")?;
        if !s.as_ref().is_some_and(|j| j.id == session_id) {
            return Err("CAPTURE_CANCELLED".into());
        }
        state
            .results
            .lock()
            .map_err(|_| "CAPTURE_BUSY")?
            .insert(session_id.clone(), (Ok(image), Instant::now()));
        *s = None;
        drop(s);
        trace.log(&app, &session_id, "result_ready");
        Ok(())
    })();
    if let Err(reason) = operation {
        finish_error(&app, &session_id, &reason);
    }
    Ok(())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureResult {
    pub resource_id: String,
    pub display_name: String,
}
#[tauri::command]
pub async fn await_screen_capture(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    session_id: String,
) -> Result<CaptureResult, String> {
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    let started = Instant::now();
    loop {
        let active = app
            .state::<CaptureState>()
            .session
            .lock()
            .map_err(|_| "CAPTURE_BUSY")?
            .as_ref()
            .is_some_and(|s| s.id == session_id);
        let result = app
            .state::<CaptureState>()
            .results
            .lock()
            .map_err(|_| "CAPTURE_BUSY")?
            .remove(&session_id);
        if let Some((result, _)) = result {
            let mut image = result?;
            let path = image.path.clone();
            let id = crate::native_tools::register_resource(
                &app.state::<crate::native_tools::NativeToolState>(),
                path.clone(),
                false,
            )
            .map_err(str::to_owned)?;
            crate::capture_files::pin(&app, id.clone(), path.clone())?;
            image.keep = true;
            image.trace.log(&app, &session_id, "result_delivered");
            return Ok(CaptureResult {
                resource_id: id,
                display_name: path.file_name().unwrap().to_string_lossy().into_owned(),
            });
        }
        if !active || started.elapsed() > Duration::from_secs(185) {
            finish_error(&app, &session_id, "CAPTURE_CANCELLED");
            return Err("CAPTURE_CANCELLED".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn trace() -> Trace {
        Trace {
            execution_id: uuid::Uuid::new_v4().to_string(),
            attempt_id: uuid::Uuid::new_v4().to_string(),
        }
    }
    #[test]
    fn failed_or_cancelled_image_only_removes_its_owned_file() {
        let root = std::env::temp_dir().join(format!("ace-capture-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let saved = root.join("user-saved.png");
        fs::write(&saved, b"user saved image").unwrap();
        assert!(Image::create(saved.clone(), trace()).is_err());
        let path = root.join("new-task-image.png");
        {
            let mut image = Image::create(path.clone(), trace()).unwrap();
            use std::io::Write;
            image
                .file
                .as_mut()
                .unwrap()
                .write_all(b"unfinished")
                .unwrap();
        }
        assert!(!path.exists());
        assert_eq!(fs::read(&saved).unwrap(), b"user saved image");
        fs::remove_file(saved).unwrap();
        fs::remove_dir(root).unwrap();
    }
    #[test]
    #[ignore = "Creates controlled test windows and captures only their pixels; explicit GUI approval"]
    fn native_region_capture_omits_hidden_selector_and_writes_valid_png() {
        use windows::Win32::UI::HiDpi::{
            SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        struct Dpi(DPI_AWARENESS_CONTEXT);
        impl Drop for Dpi {
            fn drop(&mut self) {
                unsafe {
                    SetThreadDpiAwarenessContext(self.0);
                }
            }
        }
        let old =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        assert!(!old.0.is_null());
        let _dpi = Dpi(old);
        unsafe extern "system" fn paint_window(
            hwnd: windows::Win32::Foundation::HWND,
            message: u32,
            w: windows::Win32::Foundation::WPARAM,
            l: windows::Win32::Foundation::LPARAM,
        ) -> windows::Win32::Foundation::LRESULT {
            unsafe { windows::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, message, w, l) }
        }
        use windows::{
            core::{w, PCWSTR},
            Win32::{
                Foundation::{COLORREF, HWND, RECT},
                Graphics::{
                    Dwm::DwmFlush,
                    Gdi::{CreateSolidBrush, UpdateWindow},
                },
                UI::WindowsAndMessaging::{
                    CreateWindowExW, DestroyWindow, GetWindowRect, RegisterClassW,
                    SetWindowDisplayAffinity, ShowWindow, UnregisterClassW, SW_HIDE,
                    WDA_EXCLUDEFROMCAPTURE, WNDCLASSW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
                },
            },
        };
        struct Canvas {
            hwnd: HWND,
            class: PCWSTR,
        }
        impl Drop for Canvas {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.hwnd);
                    let _ = UnregisterClassW(self.class, None);
                }
            }
        }
        let create = |class: PCWSTR, color: u32| {
            let brush = unsafe { CreateSolidBrush(COLORREF(color)) };
            assert_ne!(
                unsafe {
                    RegisterClassW(&WNDCLASSW {
                        lpszClassName: class,
                        lpfnWndProc: Some(paint_window),
                        hbrBackground: brush,
                        ..Default::default()
                    })
                },
                0
            );
            let hwnd = unsafe {
                CreateWindowExW(
                    WS_EX_TOPMOST,
                    class,
                    w!("ACE capture test"),
                    WS_POPUP | WS_VISIBLE,
                    120,
                    120,
                    220,
                    160,
                    None,
                    None,
                    None,
                    None,
                )
            }
            .unwrap();
            unsafe {
                let _ = UpdateWindow(hwnd);
            }
            Canvas { hwnd, class }
        };
        let base = create(w!("ACE.CaptureBaseTest"), 0x00bbaa66);
        let mut rect = RECT::default();
        unsafe {
            GetWindowRect(base.hwnd, &mut rect).unwrap();
            DwmFlush().unwrap();
        }
        std::thread::sleep(Duration::from_millis(150));
        let root =
            std::env::temp_dir().join(format!("ace-capture-pixels-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let read_capture = |name: &str| {
            let path = root.join(name);
            let mut image = Image::create(path.clone(), trace()).unwrap();
            crate::native_windows::capture_rect_to_file(
                image.file.as_mut().unwrap(),
                rect.left + 20,
                rect.top + 20,
                32,
                24,
            )
            .unwrap();
            image.keep = true;
            drop(image);
            let mut reader = png::Decoder::new(fs::File::open(&path).unwrap())
                .read_info()
                .unwrap();
            let mut data = vec![0; reader.output_buffer_size()];
            let info = reader.next_frame(&mut data).unwrap();
            assert_eq!((info.width, info.height), (32, 24));
            // Optional isolated fixture for lifecycle/restart verification, never a product setting.
            if name == "before.png" {
                if let Some(output) = std::env::var_os("ACE_CAPTURE_TEST_OUTPUT") {
                    let mut target = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(output)
                        .unwrap();
                    std::io::copy(&mut fs::File::open(&path).unwrap(), &mut target).unwrap();
                    target.sync_all().unwrap();
                }
            }
            fs::remove_file(path).unwrap();
            data[..info.buffer_size()].to_vec()
        };
        let before = read_capture("before.png");
        assert!(
            before
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [0x66, 0xaa, 0xbb, 0xff]),
            "test must capture only its own colored canvas; first pixel {:?}",
            &before[..4]
        );
        let selector = create(w!("ACE.CaptureSelectorTest"), 0x002244ee);
        unsafe {
            SetWindowDisplayAffinity(selector.hwnd, WDA_EXCLUDEFROMCAPTURE).unwrap();
            let _ = ShowWindow(selector.hwnd, SW_HIDE);
            DwmFlush().unwrap();
        }
        let after = read_capture("after.png");
        assert_eq!(before, after);
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn mixed_dpi_and_negative_monitor_origin_are_physical() {
        let d = Display {
            name: "test".into(),
            x: -1920,
            y: -1080,
            width: 1920,
            height: 1080,
            scale: 1.5,
        };
        assert_eq!(
            physical_region(
                &d,
                Some(Region {
                    x: 10.0,
                    y: 20.0,
                    width: 100.0,
                    height: 50.0
                })
            )
            .unwrap(),
            (-1905, -1050, 150, 75)
        );
        assert_eq!(
            physical_region(&d, None).unwrap(),
            (-1920, -1080, 1920, 1080)
        );
    }
    #[test]
    fn cross_monitor_and_invalid_regions_are_rejected() {
        let d = Display {
            name: "test".into(),
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            scale: 2.0,
        };
        for r in [
            Region {
                x: -1.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            Region {
                x: 49.0,
                y: 0.0,
                width: 2.0,
                height: 1.0,
            },
            Region {
                x: f64::NAN,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
        ] {
            assert!(physical_region(&d, Some(r)).is_err());
        }
    }
}
