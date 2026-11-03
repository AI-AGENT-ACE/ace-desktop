#[cfg(windows)]
use std::{ffi::c_void, path::Path};

#[cfg(windows)]
pub fn open_with_shell(target: &Path) -> Result<(), &'static str> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::{w, PCWSTR},
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    };
    // Use the registered file/URL handler and inspect Shell's result. Starting
    // explorer.exe alone does not establish that the target was accepted.
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(target.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    match result.0 as isize {
        value if value > 32 => Ok(()),
        5 => Err("PERMISSION_DENIED"),
        2 | 3 => Err("FILE_NOT_FOUND"),
        _ => Err("EXECUTION_FAILED"),
    }
}

#[cfg(windows)]
pub fn recycle(path: &Path) -> Result<(), &'static str> {
    crate::recycle_bin::recycle(path)
}

#[cfg(windows)]
pub fn clipboard_write(app: &tauri::AppHandle, text: &str) -> Result<(), &'static str> {
    use tauri::Manager;
    use windows::Win32::{
        Foundation::{GlobalFree, HANDLE, HWND},
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
            Ole::CF_UNICODETEXT,
        },
    };
    let utf16: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let owner = app
        .get_webview_window("main")
        .ok_or("CLIPBOARD_DENIED")?
        .hwnd()
        .map_err(|_| "CLIPBOARD_DENIED")?;
    unsafe {
        OpenClipboard(Some(HWND(owner.0))).map_err(|_| "CLIPBOARD_DENIED")?;
        let result = (|| {
            let memory =
                GlobalAlloc(GMEM_MOVEABLE, utf16.len() * 2).map_err(|_| "CLIPBOARD_DENIED")?;
            let pointer = GlobalLock(memory) as *mut u16;
            if pointer.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err("CLIPBOARD_DENIED");
            }
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), pointer, utf16.len());
            let _ = GlobalUnlock(memory);
            if EmptyClipboard().is_err()
                || SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(memory.0))).is_err()
            {
                let _ = GlobalFree(Some(memory));
                return Err("CLIPBOARD_DENIED");
            }
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
                CloseClipboard, CountClipboardFormats, GetClipboardData,
                IsClipboardFormatAvailable, OpenClipboard,
            },
            Memory::{GlobalLock, GlobalSize, GlobalUnlock},
            Ole::CF_UNICODETEXT,
        },
    };
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_err() {
            return if CountClipboardFormats() == 0 {
                Ok(String::new())
            } else {
                Err("CLIPBOARD_UNSUPPORTED_FORMAT")
            };
        }
        OpenClipboard(None).map_err(|_| "CLIPBOARD_DENIED")?;
        let result = (|| {
            let handle =
                GetClipboardData(CF_UNICODETEXT.0 as u32).map_err(|_| "CLIPBOARD_DENIED")?;
            let memory = HGLOBAL(handle.0);
            let bytes = GlobalSize(memory);
            if bytes == 0 || bytes > 131074 || !bytes.is_multiple_of(2) {
                return Err("CLIPBOARD_DENIED");
            }
            let pointer = GlobalLock(memory) as *const u16;
            if pointer.is_null() {
                return Err("CLIPBOARD_DENIED");
            }
            let mut length = 0usize;
            while length < bytes / 2 && *pointer.add(length) != 0 {
                length += 1;
            }
            let value = if length == bytes / 2 {
                Err("CLIPBOARD_DENIED")
            } else {
                String::from_utf16(std::slice::from_raw_parts(pointer, length))
                    .map_err(|_| "CLIPBOARD_DENIED")
            };
            let _ = GlobalUnlock(memory);
            value.and_then(|text| {
                if text.len() > 65536 {
                    Err("CLIPBOARD_DENIED")
                } else {
                    Ok(text)
                }
            })
        })();
        let _ = CloseClipboard();
        result
    }
}

#[cfg(windows)]
pub fn capture_rect_to_file(
    file: &mut std::fs::File,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), &'static str> {
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Gdi::{
            BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
            GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT,
            DIB_RGB_COLORS, SRCCOPY,
        },
    };
    if width == 0 || height == 0 || width as u64 * height as u64 > 64 * 1024 * 1024 {
        return Err("SCREEN_CAPTURE_FAILED");
    }
    let width = width as i32;
    let height = height as i32;
    use windows::Win32::UI::HiDpi::{
        SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    struct Awareness(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT);
    impl Drop for Awareness {
        fn drop(&mut self) {
            unsafe {
                SetThreadDpiAwarenessContext(self.0);
            }
        }
    }
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous.0.is_null() {
        return Err("SCREEN_CAPTURE_FAILED");
    }
    let _awareness = Awareness(previous);
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
            if !memory.is_invalid() {
                let _ = DeleteDC(memory);
            }
            if !bitmap.is_invalid() {
                let _ = DeleteObject(bitmap.into());
            }
            let _ = ReleaseDC(Some(HWND::default()), screen);
            return Err("SCREEN_CAPTURE_FAILED");
        }
        let old = SelectObject(memory, bitmap.into());
        if old.is_invalid() || old.0 as isize == -1 {
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(memory);
            let _ = ReleaseDC(Some(HWND::default()), screen);
            return Err("SCREEN_CAPTURE_FAILED");
        }
        let copied = BitBlt(
            memory,
            0,
            0,
            width,
            height,
            Some(screen),
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        )
        .is_ok();
        let _ = SelectObject(memory, old);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
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
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        let _ = ReleaseDC(Some(HWND::default()), screen);
        if lines != height {
            return Err("SCREEN_CAPTURE_FAILED");
        }
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        let mut encoder = png::Encoder::new(&mut *file, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        writer
            .write_image_data(&bgra)
            .map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        writer.finish().map_err(|_| "SCREEN_CAPTURE_FAILED")?;
        file.sync_all().map_err(|_| "SCREEN_CAPTURE_FAILED")
    }
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
pub fn clipboard_write(_: &tauri::AppHandle, _: &str) -> Result<(), &'static str> {
    Err("BLOCKED_BY_POLICY")
}
#[cfg(not(windows))]
pub fn clipboard_read() -> Result<String, &'static str> {
    Err("BLOCKED_BY_POLICY")
}
#[cfg(not(windows))]
pub fn capture_rect_to_file(
    _: &mut std::fs::File,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) -> Result<(), &'static str> {
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
