#[cfg(windows)]
use std::{ffi::c_void, path::Path};

#[cfg(windows)]
pub fn open_with_shell(target: &Path) -> Result<(), &'static str> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("explorer.exe")
        .arg(target)
        .creation_flags(0x08000000)
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::PermissionDenied {
                "PERMISSION_DENIED"
            } else {
                "EXECUTION_FAILED"
            }
        })
}

#[cfg(windows)]
pub fn recycle(path: &Path) -> Result<(), &'static str> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE,
        SHFILEOPSTRUCTW,
    };
    let mut value: Vec<u16> = path.as_os_str().encode_wide().collect();
    value.extend([0, 0]);
    let mut operation = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(value.as_ptr()),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT).0 as u16,
        ..Default::default()
    };
    let code = unsafe { SHFileOperationW(&mut operation) };
    if code == 0 && !operation.fAnyOperationsAborted.as_bool() {
        Ok(())
    } else {
        Err("PERMISSION_DENIED")
    }
}

#[cfg(windows)]
pub fn clipboard_write(text: &str) -> Result<(), &'static str> {
    use windows::Win32::{
        Foundation::HANDLE,
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
            Ole::CF_UNICODETEXT,
        },
    };
    let utf16: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        OpenClipboard(None).map_err(|_| "CLIPBOARD_DENIED")?;
        let result = (|| {
            EmptyClipboard().map_err(|_| "CLIPBOARD_DENIED")?;
            let memory =
                GlobalAlloc(GMEM_MOVEABLE, utf16.len() * 2).map_err(|_| "CLIPBOARD_DENIED")?;
            let pointer = GlobalLock(memory) as *mut u16;
            if pointer.is_null() {
                return Err("CLIPBOARD_DENIED");
            }
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), pointer, utf16.len());
            let _ = GlobalUnlock(memory);
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(memory.0)))
                .map_err(|_| "CLIPBOARD_DENIED")?;
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

#[cfg(windows)]
pub fn clipboard_read() -> Result<String, &'static str> {
    use windows::Win32::{
        Foundation::HGLOBAL,
        System::{
            DataExchange::{
                CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
            },
            Memory::{GlobalLock, GlobalUnlock},
            Ole::CF_UNICODETEXT,
        },
    };
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_err() {
            return Err("CLIPBOARD_DENIED");
        }
        OpenClipboard(None).map_err(|_| "CLIPBOARD_DENIED")?;
        let result = (|| {
            let handle =
                GetClipboardData(CF_UNICODETEXT.0 as u32).map_err(|_| "CLIPBOARD_DENIED")?;
            let memory = HGLOBAL(handle.0);
            let pointer = GlobalLock(memory) as *const u16;
            if pointer.is_null() {
                return Err("CLIPBOARD_DENIED");
            }
            let mut length = 0usize;
            while length < 65_536 && *pointer.add(length) != 0 {
                length += 1;
            }
            let value = String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length));
            let _ = GlobalUnlock(memory);
            Ok(value)
        })();
        let _ = CloseClipboard();
        result
    }
}

#[cfg(windows)]
pub fn capture_primary(path: &Path) -> Result<(), &'static str> {
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Gdi::{
            BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
            GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
            DIB_RGB_COLORS, SRCCOPY,
        },
        UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN},
    };
    let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    if width <= 0 || height <= 0 {
        return Err("SCREEN_CAPTURE_FAILED");
    }
    unsafe {
        let screen = GetDC(Some(HWND::default()));
        if screen.is_invalid() {
            return Err("SCREEN_CAPTURE_FAILED");
        }
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        if memory.is_invalid() || bitmap.is_invalid() {
            let _ = ReleaseDC(Some(HWND::default()), screen);
            return Err("SCREEN_CAPTURE_FAILED");
        }
        let old = SelectObject(memory, bitmap.into());
        let copied = BitBlt(memory, 0, 0, width, height, Some(screen), 0, 0, SRCCOPY).is_ok();
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut bgra = vec![0u8; width as usize * height as usize * 4];
        let lines = if copied {
            GetDIBits(
                memory,
                bitmap,
                0,
                height as u32,
                Some(bgra.as_mut_ptr() as *mut c_void),
                &mut info,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };
        let _ = SelectObject(memory, old);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        let _ = ReleaseDC(Some(HWND::default()), screen);
        if lines == 0 {
            return Err("SCREEN_CAPTURE_FAILED");
        }
        for pixel in bgra.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        let file = std::fs::File::create(path).map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        let mut encoder = png::Encoder::new(file, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(&bgra))
            .map_err(|_| "SCREEN_CAPTURE_FAILED")
    }
}

#[cfg(windows)]
pub fn control_window(process_name: &str, action: &str) -> Result<(), &'static str> {
    use windows::{
        core::BOOL,
        Win32::{
            Foundation::{HWND, LPARAM},
            UI::WindowsAndMessaging::{
                EnumWindows, SetForegroundWindow, ShowWindow, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE,
            },
        },
    };
    struct Context {
        target: String,
        found: HWND,
    }
    unsafe extern "system" fn visit(hwnd: HWND, parameter: LPARAM) -> BOOL {
        use windows::{
            core::{BOOL, PWSTR},
            Win32::{
                Foundation::CloseHandle,
                System::Threading::{
                    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
                    PROCESS_QUERY_LIMITED_INFORMATION,
                },
                UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindowVisible},
            },
        };
        let context = &mut *(parameter.0 as *mut Context);
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return BOOL(1);
        };
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let matches = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
        .is_ok()
            && Path::new(&String::from_utf16_lossy(&buffer[..size as usize]))
                .file_name()
                .and_then(|v| v.to_str())
                .is_some_and(|v| v.eq_ignore_ascii_case(&context.target));
        let _ = CloseHandle(process);
        if matches {
            context.found = hwnd;
            BOOL(0)
        } else {
            BOOL(1)
        }
    }
    let mut context = Context {
        target: process_name.into(),
        found: HWND::default(),
    };
    let _ = unsafe { EnumWindows(Some(visit), LPARAM((&mut context as *mut Context) as isize)) };
    if context.found.is_invalid() {
        return Err("WINDOW_NOT_FOUND");
    }
    unsafe {
        match action {
            "app.focus" => {
                let _ = ShowWindow(context.found, SW_RESTORE);
                if !SetForegroundWindow(context.found).as_bool() {
                    return Err("WINDOW_NOT_FOUND");
                }
            }
            "app.minimize" => {
                let _ = ShowWindow(context.found, SW_MINIMIZE);
            }
            "app.maximize" => {
                let _ = ShowWindow(context.found, SW_MAXIMIZE);
            }
            _ => return Err("BLOCKED_BY_POLICY"),
        }
    }
    Ok(())
}

#[cfg(windows)]
pub fn lock_workstation() -> Result<(), &'static str> {
    unsafe { windows::Win32::System::Shutdown::LockWorkStation().map_err(|_| "PERMISSION_DENIED") }
}

#[cfg(windows)]
pub fn set_volume(level: Option<f32>, muted: Option<bool>) -> Result<(), &'static str> {
    use windows::Win32::{
        Media::Audio::{
            eConsole, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator,
            MMDeviceEnumerator,
        },
        System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED},
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|_| "PERMISSION_DENIED")?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(|_| "PERMISSION_DENIED")?;
        let endpoint: IAudioEndpointVolume = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|_| "PERMISSION_DENIED")?;
        if let Some(value) = level {
            endpoint
                .SetMasterVolumeLevelScalar(value, std::ptr::null())
                .map_err(|_| "PERMISSION_DENIED")?;
        }
        if let Some(value) = muted {
            endpoint
                .SetMute(value, std::ptr::null())
                .map_err(|_| "PERMISSION_DENIED")?;
        }
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn open_with_shell(_: &std::path::Path) -> Result<(), &'static str> {
    Err("BLOCKED_BY_POLICY")
}
#[cfg(not(windows))]
pub fn recycle(_: &std::path::Path) -> Result<(), &'static str> {
    Err("BLOCKED_BY_POLICY")
}
#[cfg(not(windows))]
pub fn clipboard_write(_: &str) -> Result<(), &'static str> {
    Err("BLOCKED_BY_POLICY")
}
#[cfg(not(windows))]
pub fn clipboard_read() -> Result<String, &'static str> {
    Err("BLOCKED_BY_POLICY")
}
#[cfg(not(windows))]
pub fn capture_primary(_: &std::path::Path) -> Result<(), &'static str> {
    Err("BLOCKED_BY_POLICY")
}
#[cfg(not(windows))]
pub fn control_window(_: &str, _: &str) -> Result<(), &'static str> {
    Err("BLOCKED_BY_POLICY")
}
#[cfg(not(windows))]
pub fn lock_workstation() -> Result<(), &'static str> {
    Err("BLOCKED_BY_POLICY")
}
#[cfg(not(windows))]
pub fn set_volume(_: Option<f32>, _: Option<bool>) -> Result<(), &'static str> {
    Err("BLOCKED_BY_POLICY")
}
