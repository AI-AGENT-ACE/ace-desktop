//! Open URLs, settings URIs and files through their registered Windows handler.
//! Creating an Explorer process does not confirm a URL launch. ShellExecute
//! reports whether Windows accepted the operation for the registered handler.
use std::{os::windows::ffi::OsStrExt, path::Path};
use windows::{
    core::{w, PCWSTR},
    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
};

pub fn open(target: &Path) -> Result<(), &'static str> {
    let target = encode_target(target)?;
    // Keep URL/URI text verbatim. Do not turn it into a filesystem path, invoke
    // a command interpreter, or fall back to opening an unrelated folder.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    shell_result(result.0 as isize)
}

fn encode_target(target: &Path) -> Result<Vec<u16>, &'static str> {
    let mut value: Vec<u16> = target.as_os_str().encode_wide().collect();
    if value.is_empty() || value.contains(&0) {
        return Err("INVALID_ARGUMENT");
    }
    value.push(0);
    Ok(value)
}

fn shell_result(code: isize) -> Result<(), &'static str> {
    // ShellExecuteW returns a value >32 on success, not a process exit status.
    match code {
        33.. => Ok(()),
        2 | 3 => Err("FILE_NOT_FOUND"),
        5 => Err("PERMISSION_DENIED"),
        27 | 31 => Err("NO_ASSOCIATION"),
        _ => Err("EXECUTION_FAILED"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_encoding_preserves_search_query_and_settings_scheme() {
        for target in [
            "https://www.naver.com/",
            "https://www.google.com/search?q=서울+여행+정보&source=ace",
            "ms-settings:display",
            r"C:\Users\사용자\Documents\보고서.txt",
        ] {
            let encoded = encode_target(Path::new(target)).unwrap();
            assert_eq!(encoded.last(), Some(&0));
            assert_eq!(
                String::from_utf16(&encoded[..encoded.len() - 1]).unwrap(),
                target
            );
        }
    }

    #[test]
    fn invalid_targets_are_rejected_before_launch() {
        assert_eq!(encode_target(Path::new("")), Err("INVALID_ARGUMENT"));
        assert_eq!(
            encode_target(Path::new("https://example.com/\0ignored")),
            Err("INVALID_ARGUMENT")
        );
    }

    #[test]
    fn shell_failures_are_not_reported_as_success() {
        assert_eq!(shell_result(33), Ok(()));
        assert_eq!(shell_result(42), Ok(()));
        assert_eq!(shell_result(5), Err("PERMISSION_DENIED"));
        assert_eq!(shell_result(2), Err("FILE_NOT_FOUND"));
        assert_eq!(shell_result(3), Err("FILE_NOT_FOUND"));
        assert_eq!(shell_result(27), Err("NO_ASSOCIATION"));
        assert_eq!(shell_result(31), Err("NO_ASSOCIATION"));
        for code in [-1, 0, 11, 26, 28, 29, 30, 32] {
            assert_eq!(shell_result(code), Err("EXECUTION_FAILED"));
        }
    }
}
