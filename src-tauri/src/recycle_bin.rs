//! Deletion is exclusively a verified recycle operation; no permanent-delete fallback.
#[cfg(windows)]
mod implementation {
    use std::{
        os::windows::ffi::{OsStrExt, OsStringExt},
        path::Path,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    };
    use windows::{
        core::{implement, Error, PCWSTR},
        Win32::{
            Foundation::E_ABORT,
            Storage::FileSystem::GetDriveTypeW,
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL,
                COINIT_APARTMENTTHREADED,
            },
            UI::Shell::{
                FileOperation, IFileOperation, IFileOperationProgressSink,
                IFileOperationProgressSink_Impl, IShellItem, SHCreateItemFromParsingName,
                FOFX_EARLYFAILURE, FOFX_RECYCLEONDELETE, FOF_NOERRORUI, FOF_SILENT,
            },
        },
    };
    #[implement(IFileOperationProgressSink)]
    struct Sink {
        completed: Arc<AtomicBool>,
    }
    fn recycle_flags(flags: u32) -> bool {
        flags & 0x80 != 0
    }
    fn diagnostic(stage: &str, detail: impl std::fmt::Display) {
        #[cfg(debug_assertions)]
        eprintln!("[ACE recycle] stage={stage} {detail}");
        #[cfg(not(debug_assertions))]
        let _ = (stage, detail);
    }
    fn api_error(stage: &str, error: Error) -> &'static str {
        diagnostic(
            stage,
            format!(
                "hresult=0x{:08X} message={}",
                error.code().0 as u32,
                error.message()
            ),
        );
        "RECYCLE_UNAVAILABLE"
    }
    fn io_error(stage: &str, error: std::io::Error) -> &'static str {
        diagnostic(
            stage,
            format!("os_code={:?} message={error}", error.raw_os_error()),
        );
        "RECYCLE_UNAVAILABLE"
    }
    fn shell_path(path: &Path) -> Result<std::path::PathBuf, &'static str> {
        // Shell parsing does not accept canonicalize()'s verbatim drive prefix.
        // Do not reinterpret UNC/device paths or strip trailing dots/spaces.
        use std::path::{Component, Prefix};
        let normalized = match path.components().next() {
            Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::VerbatimDisk(_)) => {
                let wide: Vec<_> = path.as_os_str().encode_wide().collect();
                std::path::PathBuf::from(std::ffi::OsString::from_wide(&wide[4..]))
            }
            Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_)) => path.to_owned(),
            _ => {
                diagnostic("shell_path", "unsupported path prefix");
                return Err("RECYCLE_UNAVAILABLE");
            }
        };
        for part in normalized.components() {
            if let Component::Normal(name) = part {
                if name
                    .encode_wide()
                    .last()
                    .is_some_and(|c| c == 32 || c == 46)
                {
                    diagnostic("shell_path", "ambiguous trailing dot or space");
                    return Err("RECYCLE_UNAVAILABLE");
                }
            }
        }
        if normalized
            .canonicalize()
            .map_err(|e| io_error("shell_path_verify", e))?
            != path
                .canonicalize()
                .map_err(|e| io_error("source_path_verify", e))?
        {
            diagnostic("shell_path", "canonical target mismatch");
            return Err("RECYCLE_UNAVAILABLE");
        }
        diagnostic("shell_path", "verified same canonical target");
        Ok(normalized)
    }
    #[allow(non_snake_case, unused_variables)]
    impl IFileOperationProgressSink_Impl for Sink_Impl {
        fn StartOperations(&self) -> windows_core::Result<()> {
            Ok(())
        }
        fn FinishOperations(&self, hrresult: windows_core::HRESULT) -> windows_core::Result<()> {
            diagnostic(
                "FinishOperations",
                format!("hresult=0x{:08X}", hrresult.0 as u32),
            );
            Ok(())
        }
        fn PreRenameItem(
            &self,
            dwflags: u32,
            psiitem: windows_core::Ref<'_, IShellItem>,
            psznewname: &windows_core::PCWSTR,
        ) -> windows_core::Result<()> {
            Err(Error::from_hresult(E_ABORT))
        }
        fn PostRenameItem(
            &self,
            dwflags: u32,
            psiitem: windows_core::Ref<'_, IShellItem>,
            psznewname: &windows_core::PCWSTR,
            hrrename: windows_core::HRESULT,
            psinewlycreated: windows_core::Ref<'_, IShellItem>,
        ) -> windows_core::Result<()> {
            Ok(())
        }
        fn PreMoveItem(
            &self,
            dwflags: u32,
            psiitem: windows_core::Ref<'_, IShellItem>,
            psidestinationfolder: windows_core::Ref<'_, IShellItem>,
            psznewname: &windows_core::PCWSTR,
        ) -> windows_core::Result<()> {
            Err(Error::from_hresult(E_ABORT))
        }
        fn PostMoveItem(
            &self,
            dwflags: u32,
            psiitem: windows_core::Ref<'_, IShellItem>,
            psidestinationfolder: windows_core::Ref<'_, IShellItem>,
            psznewname: &windows_core::PCWSTR,
            hrmove: windows_core::HRESULT,
            psinewlycreated: windows_core::Ref<'_, IShellItem>,
        ) -> windows_core::Result<()> {
            Ok(())
        }
        fn PreCopyItem(
            &self,
            dwflags: u32,
            psiitem: windows_core::Ref<'_, IShellItem>,
            psidestinationfolder: windows_core::Ref<'_, IShellItem>,
            psznewname: &windows_core::PCWSTR,
        ) -> windows_core::Result<()> {
            Err(Error::from_hresult(E_ABORT))
        }
        fn PostCopyItem(
            &self,
            dwflags: u32,
            psiitem: windows_core::Ref<'_, IShellItem>,
            psidestinationfolder: windows_core::Ref<'_, IShellItem>,
            psznewname: &windows_core::PCWSTR,
            hrcopy: windows_core::HRESULT,
            psinewlycreated: windows_core::Ref<'_, IShellItem>,
        ) -> windows_core::Result<()> {
            Ok(())
        }
        fn PreDeleteItem(
            &self,
            dwflags: u32,
            psiitem: windows_core::Ref<'_, IShellItem>,
        ) -> windows_core::Result<()> {
            diagnostic(
                "PreDeleteItem",
                format!("flags=0x{dwflags:08X} recycle={}", recycle_flags(dwflags)),
            );
            if !recycle_flags(dwflags) {
                return Err(Error::from_hresult(E_ABORT));
            }
            Ok(())
        }
        fn PostDeleteItem(
            &self,
            dwflags: u32,
            psiitem: windows_core::Ref<'_, IShellItem>,
            hrdelete: windows_core::HRESULT,
            psinewlycreated: windows_core::Ref<'_, IShellItem>,
        ) -> windows_core::Result<()> {
            diagnostic(
                "PostDeleteItem",
                format!(
                    "hresult=0x{:08X} message={} recycled_item={}",
                    hrdelete.0 as u32,
                    Error::from_hresult(hrdelete).message(),
                    !psinewlycreated.is_null()
                ),
            );
            if hrdelete.is_ok() && !psinewlycreated.is_null() {
                self.completed.store(true, Ordering::Release);
                Ok(())
            } else {
                Err(Error::from_hresult(E_ABORT))
            }
        }
        fn PreNewItem(
            &self,
            dwflags: u32,
            psidestinationfolder: windows_core::Ref<'_, IShellItem>,
            psznewname: &windows_core::PCWSTR,
        ) -> windows_core::Result<()> {
            Err(Error::from_hresult(E_ABORT))
        }
        fn PostNewItem(
            &self,
            dwflags: u32,
            psidestinationfolder: windows_core::Ref<'_, IShellItem>,
            psznewname: &windows_core::PCWSTR,
            psztemplatename: &windows_core::PCWSTR,
            dwfileattributes: u32,
            hrnew: windows_core::HRESULT,
            psinewitem: windows_core::Ref<'_, IShellItem>,
        ) -> windows_core::Result<()> {
            Ok(())
        }
        fn UpdateProgress(&self, iworktotal: u32, iworksofar: u32) -> windows_core::Result<()> {
            Ok(())
        }
        fn ResetTimer(&self) -> windows_core::Result<()> {
            Ok(())
        }
        fn PauseTimer(&self) -> windows_core::Result<()> {
            Ok(())
        }
        fn ResumeTimer(&self) -> windows_core::Result<()> {
            Ok(())
        }
    }
    pub fn run(path: &Path) -> Result<(), &'static str> {
        use std::os::windows::fs::MetadataExt;
        diagnostic(
            "path",
            format!(
                "absolute={} exists={} file={}",
                path.is_absolute(),
                path.exists(),
                path.is_file()
            ),
        );
        if std::fs::symlink_metadata(path)
            .map_err(|e| io_error("metadata", e))?
            .file_attributes()
            & 0x400
            != 0
        {
            diagnostic("metadata", "reparse point rejected");
            return Err("RECYCLE_UNAVAILABLE");
        }
        if !path.is_file()
            || std::fs::symlink_metadata(path)
                .map_err(|e| io_error("metadata", e))?
                .file_type()
                .is_symlink()
        {
            diagnostic("metadata", "not a regular file or symbolic link");
            return Err("RECYCLE_UNAVAILABLE");
        }
        let root = path.ancestors().last().ok_or("RECYCLE_UNAVAILABLE")?;
        let root: Vec<_> = root.as_os_str().encode_wide().chain(Some(0)).collect();
        let drive_type = unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) };
        diagnostic(
            "GetDriveTypeW",
            format!("drive_type={drive_type} root_units={}", root.len()),
        );
        if drive_type != 3 {
            return Err("RECYCLE_UNAVAILABLE");
        }
        let shell_path = shell_path(path)?;
        let name: Vec<_> = shell_path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|error| api_error("CoInitializeEx", error))?;
            struct Apartment;
            impl Drop for Apartment {
                fn drop(&mut self) {
                    unsafe {
                        CoUninitialize();
                    }
                }
            }
            let _apartment = Apartment;
            let operation: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)
                .map_err(|error| api_error("CoCreateInstance", error))?;
            operation
                .SetOperationFlags(
                    FOFX_RECYCLEONDELETE | FOFX_EARLYFAILURE | FOF_SILENT | FOF_NOERRORUI,
                )
                .map_err(|error| api_error("SetOperationFlags", error))?;
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(name.as_ptr()), None)
                .map_err(|error| api_error("SHCreateItemFromParsingName", error))?;
            let completed = Arc::new(AtomicBool::new(false));
            let sink: IFileOperationProgressSink = Sink {
                completed: completed.clone(),
            }
            .into();
            operation
                .DeleteItem(&item, &sink)
                .map_err(|error| api_error("DeleteItem", error))?;
            operation
                .PerformOperations()
                .map_err(|error| api_error("PerformOperations", error))?;
            let aborted = operation
                .GetAnyOperationsAborted()
                .map_err(|error| api_error("GetAnyOperationsAborted", error))?
                .as_bool();
            diagnostic(
                "completion",
                format!(
                    "aborted={aborted} recycled_item_confirmed={}",
                    completed.load(Ordering::Acquire)
                ),
            );
            if aborted || !completed.load(Ordering::Acquire) {
                return Err("RECYCLE_UNAVAILABLE");
            }
        }
        Ok(())
    }
    #[cfg(test)]
    mod tests {
        #[test]
        fn shell_path_preserves_canonical_identity() {
            let file =
                std::env::temp_dir().join(format!("ace-shell-path-{}.txt", uuid::Uuid::new_v4()));
            std::fs::write(&file, b"path-only fixture").unwrap();
            let canonical = file.canonicalize().unwrap();
            let shell = super::shell_path(&canonical).unwrap();
            assert!(!shell.to_string_lossy().starts_with(r"\\?\"));
            assert_eq!(shell.canonicalize().unwrap(), canonical);
            std::fs::remove_file(file).unwrap();
        }
        #[test]
        fn shell_path_rejects_device_and_unc_paths() {
            for path in [
                r"\\server\share\file.txt",
                r"\\?\UNC\server\share\file.txt",
                r"\\.\C:\file.txt",
            ] {
                assert!(super::shell_path(std::path::Path::new(path)).is_err());
            }
        }
        #[test]
        fn non_recycle_delete_is_vetoed() {
            assert!(!super::recycle_flags(0));
            assert!(!super::recycle_flags(0x40));
            assert!(super::recycle_flags(0x80));
        }
    }
}
pub fn recycle(path: &std::path::Path) -> Result<(), &'static str> {
    #[cfg(windows)]
    {
        let path = path.to_owned();
        std::thread::spawn(move || implementation::run(&path))
            .join()
            .map_err(|_| "RECYCLE_UNAVAILABLE")?
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("RECYCLE_UNAVAILABLE")
    }
}

#[cfg(all(test, windows))]
mod integration_tests {
    #[test]
    #[ignore = "Moves only a generated test file to Windows Recycle Bin; explicit execution approval"]
    fn native_recycle_preserves_file_when_unavailable_and_never_uses_permanent_fallback() {
        let base = if std::env::var_os("ACE_RECYCLE_TEST_DESKTOP").is_some() {
            dirs::desktop_dir().unwrap().join("ACE-Native-Test")
        } else {
            std::env::temp_dir()
        };
        let root = base.join(format!("ace-recycle-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join(format!("ace-recycle-fixture-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&file, b"ACE generated recycle verification fixture").unwrap();
        eprintln!("RECYCLE_FIXTURE: {}", file.display());
        match super::recycle(&file.canonicalize().unwrap()) {
            Ok(()) => {
                assert!(!file.exists());
                eprintln!("RECYCLE_VERIFIED: generated fixture retained in Windows Recycle Bin");
            }
            Err(reason) => {
                assert_eq!(reason, "RECYCLE_UNAVAILABLE");
                assert!(file.exists(), "on failure the original must remain");
                panic!("recycle unavailable in this environment; original test file was preserved");
            }
        }
        // Keep the generated parent directory for a manual Recycle Bin restore.
    }
}
