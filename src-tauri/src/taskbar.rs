use std::{error::Error, os::windows::ffi::OsStrExt};
use tauri::{path::BaseDirectory, AppHandle, Manager, WebviewWindow};
use windows::{
    core::GUID,
    Win32::{
        Foundation::{HWND, PROPERTYKEY},
        System::Com::StructuredStorage::PROPVARIANT,
        UI::Shell::{
            PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
            SHChangeNotify, SHCNE_UPDATEITEM, SHCNF_FLUSHNOWAIT, SHCNF_PATHW,
        },
    },
};

const APP_USER_MODEL: GUID = GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3);

fn set_string(store: &IPropertyStore, pid: u32, value: &str) -> windows::core::Result<()> {
    let key = PROPERTYKEY {
        fmtid: APP_USER_MODEL,
        pid,
    };
    let variant = PROPVARIANT::from(value);
    unsafe { store.SetValue(&key, &variant) }
}

pub fn configure(app: &AppHandle, window: &WebviewWindow) -> Result<(), Box<dyn Error>> {
    let icon = app
        .path()
        .resolve("ace-taskbar.ico", BaseDirectory::Resource)?;
    if !icon.is_file() {
        return Err(format!("ACE taskbar icon resource is missing: {}", icon.display()).into());
    }
    let executable = std::env::current_exe()?;
    // Shell icon locations use ordinary Win32 paths, not canonical verbatim prefixes.
    let icon_path = icon.to_string_lossy();
    let shell_icon_path = if let Some(path) = icon_path.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{path}")
    } else {
        icon_path
            .strip_prefix("\\\\?\\")
            .unwrap_or(&icon_path)
            .to_owned()
    };
    let icon_resource = format!("{shell_icon_path},0");
    let command = format!("\"{}\"", executable.display());
    let hwnd = HWND(window.hwnd()?.0);
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd)?;
        // Set relaunch details before assigning the explicit window identity.
        set_string(&store, 2, &command)?;
        set_string(&store, 3, &icon_resource)?;
        set_string(&store, 4, "ACE")?;
        set_string(&store, 5, "com.ace.desktop")?;
        store.Commit()?;
        for path in [&executable, &icon] {
            let text: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            SHChangeNotify(
                SHCNE_UPDATEITEM,
                SHCNF_PATHW | SHCNF_FLUSHNOWAIT,
                Some(text.as_ptr().cast()),
                None,
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::{
        core::{w, BSTR},
        Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WINDOW_STYLE,
        },
    };

    #[test]
    fn windows_taskbar_store_retains_explicit_ace_icon() {
        unsafe {
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("ACE property test"),
                WINDOW_STYLE(0),
                0,
                0,
                1,
                1,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            let outcome = (|| -> windows::core::Result<()> {
                let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd)?;
                set_string(&store, 2, "\"C:\\ACE\\ACE.exe\"")?;
                set_string(&store, 3, "C:\\ACE\\ace-taskbar.ico,0")?;
                set_string(&store, 4, "ACE")?;
                set_string(&store, 5, "com.ace.desktop")?;
                store.Commit()?;
                for (pid, expected) in [
                    (3, "C:\\ACE\\ace-taskbar.ico,0"),
                    (4, "ACE"),
                    (5, "com.ace.desktop"),
                ] {
                    let key = PROPERTYKEY {
                        fmtid: APP_USER_MODEL,
                        pid,
                    };
                    let value = store.GetValue(&key)?;
                    let actual = BSTR::try_from(&value)?.to_string();
                    assert_eq!(actual, expected);
                }
                Ok(())
            })();
            DestroyWindow(hwnd).unwrap();
            outcome.unwrap();
        }
    }
}
