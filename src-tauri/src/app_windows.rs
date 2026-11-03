//! Only opaque IDs cross IPC; snapshots bind action, process lifetime and window identity.
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
#[derive(Clone)]
struct Target {
    handle: isize,
    pid: u32,
    thread_id: u32,
    created: u64,
    window_nonce: isize,
    process_path: String,
    process_name: String,
    title: String,
    action: String,
    issued: Instant,
}
#[derive(Default)]
pub struct WindowTargets {
    targets: Mutex<HashMap<String, Target>>,
    closing: Mutex<HashMap<String, Target>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub target_id: String,
    pub app_name: String,
    pub title: String,
}
pub enum Outcome {
    Done,
    CloseRequested(String),
    Select(Vec<Choice>),
}
pub fn approval_label(state: &WindowTargets, id: &str) -> Result<String, &'static str> {
    let targets = state.targets.lock().map_err(|_| "EXECUTION_FAILED")?;
    let target = targets.get(id).ok_or("WINDOW_TARGET_STALE")?;
    Ok(format!("{} · {}", target.process_name, target.title))
}
pub fn approval_snapshot(state: &WindowTargets, id: &str) -> Result<String, &'static str> {
    let targets = state.targets.lock().map_err(|_| "EXECUTION_FAILED")?;
    let target = targets.get(id).ok_or("WINDOW_TARGET_STALE")?;
    if target.action != "app.close" || target.issued.elapsed() >= Duration::from_secs(120) {
        return Err("WINDOW_TARGET_STALE");
    }
    #[cfg(windows)]
    if identity(target.handle, false)?
        != (
            target.pid,
            target.thread_id,
            target.created,
            target.process_path.clone(),
            target.window_nonce,
        )
    {
        return Err("WINDOW_TARGET_STALE");
    }
    Ok(format!(
        "{}:{}:{}:{}",
        target.pid, target.created, target.handle, target.window_nonce
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseObservation {
    pub closed: bool,
    pub message: String,
}

#[tauri::command]
pub async fn observe_window_close(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    ticket: String,
) -> Result<CloseObservation, String> {
    use tauri::Manager;
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    let target = app
        .state::<WindowTargets>()
        .closing
        .lock()
        .map_err(|_| "EXECUTION_FAILED")?
        .remove(&ticket)
        .ok_or("WINDOW_TARGET_STALE")?;
    if target.issued.elapsed() > Duration::from_secs(120) {
        return Err("WINDOW_TARGET_STALE".into());
    }
    #[cfg(windows)]
    {
        Ok(observe_target(&target, Duration::from_secs(5)).await)
    }
    #[cfg(not(windows))]
    {
        Err("DESKTOP_REQUIRED".into())
    }
}
#[cfg(windows)]
async fn observe_target(target: &Target, wait: Duration) -> CloseObservation {
    let deadline = Instant::now() + wait;
    loop {
        if original_window_gone(target) {
            return CloseObservation {
                closed: true,
                message: "선택한 창이 종료되었습니다.".into(),
            };
        }
        if Instant::now() >= deadline {
            return CloseObservation {
                closed: false,
                message: "저장 확인 또는 사용자 조작이 필요합니다".into(),
            };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
#[cfg(windows)]
fn original_window_gone(target: &Target) -> bool {
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsWindow};
    if !unsafe { IsWindow(Some(HWND(target.handle as *mut _))) }.as_bool() {
        return true;
    }
    identity(target.handle, false).is_ok_and(|v| {
        v != (
            target.pid,
            target.thread_id,
            target.created,
            target.process_path.clone(),
            target.window_nonce,
        )
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn close_preview_orders_validation_focus_wait_revalidation_and_close() {
        let events = std::cell::RefCell::new(Vec::new());
        let result = close_after_preview(
            || {
                events.borrow_mut().push("validate");
                Ok(())
            },
            || {
                events.borrow_mut().push("focus");
                true
            },
            |duration| {
                assert_eq!(duration, Duration::from_millis(400));
                events.borrow_mut().push("wait");
            },
            || {
                events.borrow_mut().push("foreground");
                true
            },
            || {
                events.borrow_mut().push("close");
                Ok(())
            },
        );
        assert_eq!(result, Ok(()));
        assert_eq!(
            *events.borrow(),
            [
                "validate",
                "focus",
                "wait",
                "validate",
                "foreground",
                "close"
            ]
        );
    }
    #[test]
    fn close_preview_aborts_on_focus_failure_or_focus_loss() {
        for initial_focus in [false, true] {
            let result = close_after_preview(
                || Ok(()),
                || initial_focus,
                |_| assert!(initial_focus),
                || false,
                || panic!("must not close a background window"),
            );
            assert_eq!(result, Err("WINDOW_FOCUS_FAILED"));
        }
    }
    #[test]
    fn close_preview_aborts_if_target_changes_during_preview() {
        let mut validations = 0;
        let result = close_after_preview(
            || {
                validations += 1;
                if validations == 2 {
                    Err("WINDOW_TARGET_STALE")
                } else {
                    Ok(())
                }
            },
            || true,
            |_| {},
            || true,
            || panic!("must not close a replaced window"),
        );
        assert_eq!(result, Err("WINDOW_TARGET_STALE"));
    }
    fn target(action: &str) -> Target {
        Target {
            handle: 0,
            pid: 0,
            thread_id: 0,
            created: 0,
            window_nonce: 1,
            process_path: "invalid.exe".into(),
            process_name: "invalid.exe".into(),
            title: "Test window".into(),
            action: action.into(),
            issued: Instant::now(),
        }
    }
    #[test]
    fn fabricated_id_never_falls_back_to_another_window() {
        let s = WindowTargets::default();
        assert!(matches!(
            run(&s, "invalid.exe", "test", "app.close", Some("forged")),
            Err("WINDOW_TARGET_STALE")
        ));
    }
    #[test]
    fn action_and_process_substitution_are_rejected_and_id_is_consumed() {
        let s = WindowTargets::default();
        s.targets
            .lock()
            .unwrap()
            .insert("id".into(), target("app.minimize"));
        assert!(matches!(
            run(&s, "invalid.exe", "test", "app.close", Some("id")),
            Err("INVALID_ARGUMENT")
        ));
        assert!(s.targets.lock().unwrap().is_empty());
    }
    #[test]
    fn expired_and_disappeared_windows_never_select_another_target() {
        for expired in [true, false] {
            let s = WindowTargets::default();
            let mut t = target("app.close");
            if expired {
                t.issued = Instant::now() - Duration::from_secs(121);
            }
            s.targets.lock().unwrap().insert("id".into(), t);
            assert!(matches!(
                run(&s, "invalid.exe", "test", "app.close", Some("id")),
                Err("WINDOW_TARGET_STALE")
            ));
        }
    }

    #[test]
    #[ignore = "Creates a temporary Windows window that requires manual close; explicit GUI approval"]
    fn native_close_request_and_actual_close_have_distinct_results() {
        use windows::{
            core::w,
            Win32::{
                Foundation::{HWND, LPARAM, LRESULT, WPARAM},
                UI::WindowsAndMessaging::{
                    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, PeekMessageW,
                    RegisterClassW, TranslateMessage, UnregisterClassW, MSG, PM_REMOVE,
                    WINDOW_EX_STYLE, WM_CLOSE, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
                },
            },
        };
        unsafe extern "system" fn keep_open(
            hwnd: HWND,
            message: u32,
            w: WPARAM,
            l: LPARAM,
        ) -> LRESULT {
            if message == WM_CLOSE {
                LRESULT(0)
            } else {
                unsafe { DefWindowProcW(hwnd, message, w, l) }
            }
        }
        let class = w!("ACE.CloseVerificationTest");
        assert_ne!(
            unsafe {
                RegisterClassW(&WNDCLASSW {
                    lpszClassName: class,
                    lpfnWndProc: Some(keep_open),
                    ..Default::default()
                })
            },
            0
        );
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                w!("ACE test needs user interaction"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                80,
                80,
                300,
                180,
                None,
                None,
                None,
                None,
            )
        }
        .unwrap();
        struct Cleanup(HWND);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.0);
                    let _ = UnregisterClassW(w!("ACE.CloseVerificationTest"), None);
                }
            }
        }
        let _cleanup = Cleanup(hwnd);
        let process = std::env::current_exe()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let state = WindowTargets::default();
        let Outcome::Select(choices) = run(&state, &process, "test", "app.close", None).unwrap()
        else {
            panic!("selection must not execute before approval");
        };
        assert_eq!(choices.len(), 1);
        let Outcome::CloseRequested(ticket) = run(
            &state,
            &process,
            "test",
            "app.close",
            Some(&choices[0].target_id),
        )
        .unwrap() else {
            panic!("expected close request only");
        };
        let target = state.closing.lock().unwrap().remove(&ticket).unwrap();
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let pending =
            tauri::async_runtime::block_on(observe_target(&target, Duration::from_millis(100)));
        assert!(!pending.closed);
        assert_eq!(pending.message, "저장 확인 또는 사용자 조작이 필요합니다");
        unsafe {
            DestroyWindow(hwnd).unwrap();
        }
        let closed =
            tauri::async_runtime::block_on(observe_target(&target, Duration::from_millis(100)));
        assert!(closed.closed);
    }

    #[test]
    #[ignore = "Creates three temporary native windows; run explicitly with GUI execution approval"]
    fn selected_window_is_the_only_window_minimized_focused_and_closed() {
        use windows::{
            core::w,
            Win32::{
                Foundation::HWND,
                UI::WindowsAndMessaging::{
                    CreateWindowExW, DestroyWindow, DispatchMessageW, GetForegroundWindow,
                    IsIconic, IsWindow, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
                    WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
                },
            },
        };
        struct Windows(Vec<HWND>);
        impl Drop for Windows {
            fn drop(&mut self) {
                for hwnd in &self.0 {
                    unsafe {
                        if IsWindow(Some(*hwnd)).as_bool() {
                            let _ = DestroyWindow(*hwnd);
                        }
                    }
                }
            }
        }
        let windows = Windows(vec![
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("STATIC"),
                    w!("ACE test window A"),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    80,
                    80,
                    240,
                    160,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap()
            },
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("STATIC"),
                    w!("ACE test window B"),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    340,
                    80,
                    240,
                    160,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap()
            },
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("STATIC"),
                    w!("ACE test window C"),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    600,
                    80,
                    240,
                    160,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap()
            },
        ]);
        let process = std::env::current_exe()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let state = WindowTargets::default();
        let select_b = |action: &str| {
            let Outcome::Select(choices) = run(&state, &process, "test", action, None).unwrap()
            else {
                panic!("three windows must require selection");
            };
            assert_eq!(choices.len(), 3);
            choices
                .into_iter()
                .find(|c| c.title == "ACE test window B")
                .unwrap()
                .target_id
        };
        let id = select_b("app.minimize");
        assert!(matches!(
            run(&state, &process, "test", "app.minimize", Some(&id)),
            Ok(Outcome::Done)
        ));
        unsafe {
            assert!(!IsIconic(windows.0[0]).as_bool());
            assert!(IsIconic(windows.0[1]).as_bool());
            assert!(!IsIconic(windows.0[2]).as_bool());
        }
        let id = select_b("app.focus");
        match run(&state, &process, "test", "app.focus", Some(&id)) {
            Ok(Outcome::Done) => unsafe {
                assert_eq!(GetForegroundWindow(), windows.0[1]);
            },
            Err("PERMISSION_DENIED") => {
                // Windows may refuse foreground activation even for a valid target.
                // Validate the refusal explicitly rather than disabling foreground policy.
                eprintln!("FOCUS_POLICY_DENIED: Windows refused activation; minimize, selected close and stale-target assertions continue.");
            }
            _ => panic!("unexpected focus result"),
        }
        unsafe {
            assert!(!IsIconic(windows.0[1]).as_bool());
        }
        let id = select_b("app.close");
        assert!(matches!(
            run(&state, &process, "test", "app.close", Some(&id)),
            Ok(Outcome::CloseRequested(_))
        ));
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            assert!(IsWindow(Some(windows.0[0])).as_bool());
            assert!(!IsWindow(Some(windows.0[1])).as_bool());
            assert!(IsWindow(Some(windows.0[2])).as_bool());
        }
        assert!(matches!(
            run(&state, &process, "test", "app.close", Some(&id)),
            Err("WINDOW_TARGET_STALE")
        ));
        let Outcome::Select(choices) = run(&state, &process, "test", "app.close", None).unwrap()
        else {
            panic!("two remaining windows require selection");
        };
        let id = choices
            .into_iter()
            .find(|c| c.title == "ACE test window A")
            .unwrap()
            .target_id;
        unsafe {
            DestroyWindow(windows.0[0]).unwrap();
        }
        assert!(matches!(
            run(&state, &process, "test", "app.close", Some(&id)),
            Err("WINDOW_TARGET_STALE")
        ));
        unsafe {
            assert!(IsWindow(Some(windows.0[2])).as_bool());
        }
    }
}

#[cfg(windows)]
fn identity(
    handle: isize,
    bind_window: bool,
) -> Result<(u32, u32, u64, String, isize), &'static str> {
    use windows::{
        core::{w, PWSTR},
        Win32::{
            Foundation::{CloseHandle, FILETIME, HANDLE, HWND},
            System::Threading::{
                GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
            UI::WindowsAndMessaging::{GetPropW, GetWindowThreadProcessId, IsWindow, SetPropW},
        },
    };
    unsafe {
        let hwnd = HWND(handle as *mut _);
        if !IsWindow(Some(hwnd)).as_bool() {
            return Err("WINDOW_TARGET_STALE");
        }
        let mut pid = 0;
        let thread_id = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            .map_err(|_| "WINDOW_TARGET_STALE")?;
        let result = (|| {
            let (mut created, mut exit, mut kernel, mut user) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            GetProcessTimes(process, &mut created, &mut exit, &mut kernel, &mut user)
                .map_err(|_| "WINDOW_TARGET_STALE")?;
            let mut text = [0u16; 32768];
            let mut length = text.len() as u32;
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(text.as_mut_ptr()),
                &mut length,
            )
            .map_err(|_| "WINDOW_TARGET_STALE")?;
            let name = w!("ACE.WindowIdentity.v1");
            let mut nonce = GetPropW(hwnd, name).0 as isize;
            if nonce == 0 && bind_window {
                nonce =
                    (u64::from_le_bytes(uuid::Uuid::new_v4().as_bytes()[..8].try_into().unwrap())
                        & 0x7fff_ffff_ffff_ffff)
                        .max(1) as isize;
                SetPropW(hwnd, name, Some(HANDLE(nonce as *mut _)))
                    .map_err(|_| "PERMISSION_DENIED")?;
            }
            Ok((
                pid,
                thread_id,
                ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64,
                String::from_utf16_lossy(&text[..length as usize]),
                nonce,
            ))
        })();
        let _ = CloseHandle(process);
        result
    }
}
#[cfg(windows)]
fn focus_visible_window(handle: isize) -> bool {
    use windows::Win32::{
        Foundation::HWND,
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, IsIconic, SetForegroundWindow,
            ShowWindow, ShowWindowAsync, SW_RESTORE,
        },
    };
    unsafe {
        let hwnd = HWND(handle as *mut _);
        let minimized = IsIconic(hwnd).as_bool();
        if GetForegroundWindow() == hwnd && !minimized {
            if cfg!(debug_assertions) {
                eprintln!("[WindowFocus] stage=already_foreground close_sent=false");
            }
            return true;
        }
        if minimized {
            if GetWindowThreadProcessId(hwnd, None) == GetCurrentThreadId() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            } else {
                let _ = ShowWindowAsync(hwnd, SW_RESTORE);
            }
        }
        let requested = SetForegroundWindow(hwnd).as_bool();
        let deadline = Instant::now() + Duration::from_millis(700);
        loop {
            let visible = GetForegroundWindow() == hwnd && !IsIconic(hwnd).as_bool();
            if visible || Instant::now() >= deadline {
                if cfg!(debug_assertions) {
                    eprintln!("[WindowFocus] stage=activation request_return={requested} minimized_before={minimized} foreground_verified={visible} close_sent=false");
                }
                return visible;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[cfg(windows)]
fn close_after_preview(
    mut validate: impl FnMut() -> Result<(), &'static str>,
    focus: impl FnOnce() -> bool,
    wait: impl FnOnce(Duration),
    foreground: impl FnOnce() -> bool,
    close: impl FnOnce() -> Result<(), &'static str>,
) -> Result<(), &'static str> {
    validate()?;
    if !focus() {
        return Err("WINDOW_FOCUS_FAILED");
    }
    wait(Duration::from_millis(400));
    validate()?;
    if !foreground() {
        return Err("WINDOW_FOCUS_FAILED");
    }
    close()
}

#[cfg(windows)]
pub fn run(
    state: &WindowTargets,
    process_name: &str,
    app_name: &str,
    action: &str,
    selected: Option<&str>,
) -> Result<Outcome, &'static str> {
    use windows::{
        core::BOOL,
        Win32::{
            Foundation::{HWND, LPARAM, WPARAM},
            UI::WindowsAndMessaging::{
                EnumWindows, GetForegroundWindow, GetWindowTextW, IsIconic, IsWindowVisible,
                PostMessageW, ShowWindow, SW_MAXIMIZE, SW_MINIMIZE, WM_CLOSE,
            },
        },
    };
    let mut targets = state.targets.lock().map_err(|_| "EXECUTION_FAILED")?;
    targets.retain(|_, t| t.issued.elapsed() < Duration::from_secs(120));
    if let Some(id) = selected {
        let target = targets.remove(id).ok_or("WINDOW_TARGET_STALE")?;
        drop(targets);
        if !target.process_name.eq_ignore_ascii_case(process_name) || target.action != action {
            return Err("INVALID_ARGUMENT");
        }
        if identity(target.handle, false)?
            != (
                target.pid,
                target.thread_id,
                target.created,
                target.process_path.clone(),
                target.window_nonce,
            )
        {
            return Err("WINDOW_TARGET_STALE");
        }
        unsafe {
            let hwnd = HWND(target.handle as *mut _);
            match action {
                "app.close" => {
                    let id = uuid::Uuid::new_v4().to_string();
                    let mut closing = state.closing.lock().map_err(|_| "EXECUTION_FAILED")?;
                    closing.retain(|_, t| t.issued.elapsed() < Duration::from_secs(120));
                    if closing.len() >= 100 {
                        return Err("EXECUTION_FAILED");
                    }
                    close_after_preview(
                        || {
                            if target.issued.elapsed() >= Duration::from_secs(120)
                                || identity(target.handle, false)?
                                    != (
                                        target.pid,
                                        target.thread_id,
                                        target.created,
                                        target.process_path.clone(),
                                        target.window_nonce,
                                    )
                            {
                                return Err("WINDOW_TARGET_STALE");
                            }
                            Ok(())
                        },
                        || focus_visible_window(target.handle),
                        std::thread::sleep,
                        || GetForegroundWindow() == hwnd && !IsIconic(hwnd).as_bool(),
                        || {
                            let result = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0))
                                .map_err(|_| "PERMISSION_DENIED");
                            if cfg!(debug_assertions) {
                                eprintln!(
                                    "[WindowClose] stage=normal_close_request sent={}",
                                    result.is_ok()
                                );
                            }
                            result
                        },
                    )?;
                    closing.insert(id.clone(), target);
                    return Ok(Outcome::CloseRequested(id));
                }
                "app.focus" => {
                    if !focus_visible_window(target.handle) {
                        return Err("PERMISSION_DENIED");
                    }
                }
                "app.minimize" => {
                    let _ = ShowWindow(hwnd, SW_MINIMIZE);
                }
                "app.maximize" => {
                    let _ = ShowWindow(hwnd, SW_MAXIMIZE);
                }
                _ => return Err("BLOCKED_BY_POLICY"),
            }
        }
        return Ok(Outcome::Done);
    }
    struct Context {
        process: String,
        inaccessible: bool,
        found: Vec<(isize, u32, u32, u64, String, isize, String)>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, parameter: LPARAM) -> BOOL {
        let context = &mut *(parameter.0 as *mut Context);
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        if let Ok((_, _, _, path, _)) = identity(hwnd.0 as isize, false) {
            if std::path::Path::new(&path)
                .file_name()
                .and_then(|v| v.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(&context.process))
            {
                let mut buffer = [0u16; 512];
                let length = GetWindowTextW(hwnd, &mut buffer).max(0) as usize;
                if length > 0 {
                    let Ok((pid, thread_id, created, path, nonce)) =
                        identity(hwnd.0 as isize, true)
                    else {
                        context.inaccessible = true;
                        return BOOL(1);
                    };
                    context.found.push((
                        hwnd.0 as isize,
                        pid,
                        thread_id,
                        created,
                        path,
                        nonce,
                        String::from_utf16_lossy(&buffer[..length]),
                    ));
                }
            }
        }
        BOOL(1)
    }
    let mut context = Context {
        process: process_name.into(),
        inaccessible: false,
        found: Vec::new(),
    };
    unsafe { EnumWindows(Some(visit), LPARAM((&mut context as *mut Context) as isize)) }
        .map_err(|_| "EXECUTION_FAILED")?;
    if context.inaccessible {
        return Err("PERMISSION_DENIED");
    }
    if context.found.is_empty() {
        return Err("WINDOW_NOT_FOUND");
    }
    if context.found.len() > 100 || targets.len() + context.found.len() > 1000 {
        return Err("EXECUTION_FAILED");
    }
    let choices: Vec<_> = context
        .found
        .into_iter()
        .map(
            |(handle, pid, thread_id, created, path, window_nonce, title)| {
                let id = uuid::Uuid::new_v4().to_string();
                targets.insert(
                    id.clone(),
                    Target {
                        handle,
                        pid,
                        thread_id,
                        created,
                        window_nonce,
                        process_path: path,
                        process_name: process_name.into(),
                        title: title.clone(),
                        action: action.into(),
                        issued: Instant::now(),
                    },
                );
                Choice {
                    target_id: id,
                    app_name: app_name.into(),
                    title,
                }
            },
        )
        .collect();
    if choices.len() == 1 && action != "app.close" {
        let id = choices[0].target_id.clone();
        drop(targets);
        return run(state, process_name, app_name, action, Some(&id));
    }
    Ok(Outcome::Select(choices))
}
#[cfg(not(windows))]
pub fn run(
    _: &WindowTargets,
    _: &str,
    _: &str,
    _: &str,
    _: Option<&str>,
) -> Result<Outcome, &'static str> {
    Err("DESKTOP_REQUIRED")
}
