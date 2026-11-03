//! Fail closed when the filesystem cannot provide isolated, atomic replacement.
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
#[derive(Clone, Debug, PartialEq)]
struct Fingerprint {
    size: u64,
    hash: [u8; 32],
    modified: std::time::SystemTime,
    identity: (u64, u64),
}
pub(crate) fn approval_fingerprint(path: &Path) -> Result<String, &'static str> {
    Ok(format!("{:?}", fingerprint(&mut read_locked(path)?)?))
}
fn fingerprint(file: &mut File) -> Result<Fingerprint, &'static str> {
    let meta = file.metadata().map_err(|_| "FILE_CONFIRMATION_STALE")?;
    if !meta.is_file() || meta.len() > 512 * 1024 * 1024 {
        return Err("SAFE_OVERWRITE_UNAVAILABLE");
    }
    #[cfg(windows)]
    let identity = {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION},
        };
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }
            .map_err(|_| "SAFE_OVERWRITE_UNAVAILABLE")?;
        if info.nNumberOfLinks != 1 || info.dwFileAttributes & 0x400 != 0 {
            return Err("SAFE_OVERWRITE_UNAVAILABLE");
        }
        (
            info.dwVolumeSerialNumber as u64,
            ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
        )
    };
    #[cfg(not(windows))]
    let identity = (0, 0);
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "EXECUTION_FAILED")?;
    let mut hasher = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let count = file.read(&mut bytes).map_err(|_| "EXECUTION_FAILED")?;
        if count == 0 {
            break;
        }
        hasher.update(&bytes[..count]);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "EXECUTION_FAILED")?;
    Ok(Fingerprint {
        size: meta.len(),
        hash: hasher.finalize().into(),
        modified: meta.modified().map_err(|_| "SAFE_OVERWRITE_UNAVAILABLE")?,
        identity,
    })
}
fn read_locked(path: &Path) -> Result<File, &'static str> {
    if fs::symlink_metadata(path)
        .map_err(|_| "FILE_NOT_FOUND")?
        .file_type()
        .is_symlink()
    {
        return Err("PATH_NOT_ALLOWED");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if fs::symlink_metadata(path)
            .map_err(|_| "FILE_NOT_FOUND")?
            .file_attributes()
            & 0x400
            != 0
        {
            return Err("SAFE_OVERWRITE_UNAVAILABLE");
        }
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1);
    }
    options.open(path).map_err(|_| "PERMISSION_DENIED")
}
struct Approval {
    approved: bool,
    tool: String,
    args: Value,
    source: PathBuf,
    target: PathBuf,
    source_state: Fingerprint,
    target_state: Fingerprint,
    expires: Instant,
}
#[derive(Default)]
pub struct FileApprovals {
    approvals: Mutex<HashMap<String, Approval>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub approval_id: String,
    pub source: String,
    pub target: String,
    pub source_size: u64,
    pub target_size: u64,
    pub expires_in_seconds: u64,
}
pub enum Outcome {
    Done,
    Conflict(Conflict),
}
fn authorize(state: &FileApprovals, request_id: &str) -> Result<String, &'static str> {
    let mut approval = state
        .approvals
        .lock()
        .map_err(|_| "EXECUTION_FAILED")?
        .remove(request_id)
        .ok_or("FILE_CONFIRMATION_STALE")?;
    if approval.approved || approval.expires <= Instant::now() {
        return Err("FILE_CONFIRMATION_STALE");
    }
    if fingerprint(&mut read_locked(&approval.source)?)? != approval.source_state
        || fingerprint(&mut read_locked(&approval.target)?)? != approval.target_state
    {
        return Err("FILE_CONFIRMATION_STALE");
    }
    approval.approved = true;
    let id = uuid::Uuid::new_v4().to_string();
    state
        .approvals
        .lock()
        .map_err(|_| "EXECUTION_FAILED")?
        .insert(id.clone(), approval);
    Ok(id)
}
#[tauri::command]
pub fn authorize_file_overwrite(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    request_id: String,
) -> Result<String, String> {
    use tauri::Manager;
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    authorize(&app.state::<FileApprovals>(), &request_id).map_err(str::to_owned)
}
#[tauri::command]
pub fn discard_file_approval(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    approval_id: String,
) -> Result<(), String> {
    use tauri::Manager;
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    app.state::<FileApprovals>()
        .approvals
        .lock()
        .map_err(|_| "EXECUTION_FAILED")?
        .remove(&approval_id);
    Ok(())
}
pub fn execute(
    state: &FileApprovals,
    tool: &str,
    args: &Value,
    source: &Path,
    target: &Path,
    approval: Option<&str>,
) -> Result<Outcome, &'static str> {
    if !["file.copy", "file.move", "file.rename"].contains(&tool) {
        return Err("INVALID_ARGUMENT");
    }
    let mut bound_args = args.clone();
    bound_args
        .as_object_mut()
        .ok_or("INVALID_ARGUMENT")?
        .remove("overwriteToken");
    if let Some(token) = approval {
        let record = state
            .approvals
            .lock()
            .map_err(|_| "EXECUTION_FAILED")?
            .remove(token)
            .ok_or("FILE_CONFIRMATION_STALE")?;
        if !record.approved
            || record.expires <= Instant::now()
            || record.tool != tool
            || record.args != bound_args
            || record.source != source
            || record.target != target
        {
            return Err("FILE_CONFIRMATION_STALE");
        }
        transactional_replace(
            tool,
            source,
            target,
            &record.source_state,
            &record.target_state,
            #[cfg(test)]
            false,
        )?;
        return Ok(Outcome::Done);
    }
    let mut input = read_locked(source)?;
    if target.exists() {
        let mut output = read_locked(target)?;
        let source_state = fingerprint(&mut input)?;
        let target_state = fingerprint(&mut output)?;
        if source == target || source_state.identity == target_state.identity {
            return Err("INVALID_ARGUMENT");
        }
        let id = uuid::Uuid::new_v4().to_string();
        let info = Conflict {
            approval_id: id.clone(),
            source: source.to_string_lossy().into_owned(),
            target: target.to_string_lossy().into_owned(),
            source_size: source_state.size,
            target_size: target_state.size,
            expires_in_seconds: 120,
        };
        let mut approvals = state.approvals.lock().map_err(|_| "EXECUTION_FAILED")?;
        approvals.retain(|_, a| a.expires > Instant::now());
        if approvals.len() >= 128 {
            return Err("EXECUTION_FAILED");
        }
        approvals.insert(
            id,
            Approval {
                approved: false,
                tool: tool.into(),
                args: bound_args,
                source: source.into(),
                target: target.into(),
                source_state,
                target_state,
                expires: Instant::now() + Duration::from_secs(120),
            },
        );
        return Ok(Outcome::Conflict(info));
    }
    if tool == "file.copy" {
        let staged = target
            .parent()
            .ok_or("PATH_NOT_ALLOWED")?
            .join(format!(".ace-copy-{}.tmp", uuid::Uuid::new_v4()));
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&staged)
            .map_err(|_| "PERMISSION_DENIED")?;
        let mut cleanup = Temporary {
            path: staged.clone(),
            committed: false,
        };
        // Retain Windows alternate streams, including downloaded-file zone metadata.
        // The destination here is only our newly reserved staging file.
        fs::copy(source, &staged).map_err(|_| "EXECUTION_FAILED")?;
        output.flush().map_err(|_| "EXECUTION_FAILED")?;
        output.sync_all().map_err(|_| "EXECUTION_FAILED")?;
        drop(output);
        rename_no_replace(&staged, target)?;
        cleanup.committed = true;
    } else {
        drop(input);
        rename_no_replace(source, target)?;
    }
    Ok(Outcome::Done)
}
struct Temporary {
    path: PathBuf,
    committed: bool,
}
impl Drop for Temporary {
    fn drop(&mut self) {
        if !self.committed {
            #[cfg(windows)]
            if let Ok(metadata) = fs::metadata(&self.path) {
                if metadata.permissions().readonly() {
                    use std::os::windows::{ffi::OsStrExt, fs::MetadataExt};
                    use windows::{
                        core::PCWSTR,
                        Win32::Storage::FileSystem::{
                            SetFileAttributesW, FILE_FLAGS_AND_ATTRIBUTES,
                        },
                    };
                    let name: Vec<_> = self.path.as_os_str().encode_wide().chain(Some(0)).collect();
                    let _ = unsafe {
                        SetFileAttributesW(
                            PCWSTR(name.as_ptr()),
                            FILE_FLAGS_AND_ATTRIBUTES(metadata.file_attributes() & !1),
                        )
                    };
                }
            }
            let _ = fs::remove_file(&self.path);
        }
    }
}
#[cfg(windows)]
fn rename_no_replace(source: &Path, target: &Path) -> Result<(), &'static str> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH},
    };
    let source: Vec<_> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<_> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|_| "DESTINATION_EXISTS")
}
#[cfg(not(windows))]
fn rename_no_replace(_: &Path, _: &Path) -> Result<(), &'static str> {
    Err("SAFE_OVERWRITE_UNAVAILABLE")
}

#[cfg(windows)]
fn transactional_replace(
    tool: &str,
    source: &Path,
    target: &Path,
    source_state: &Fingerprint,
    target_state: &Fingerprint,
    #[cfg(test)] abort_before_commit: bool,
) -> Result<(), &'static str> {
    plain_streams_only(source)?;
    plain_streams_only(target)?;
    use std::os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    };
    use windows::{
        core::{w, PCWSTR},
        Win32::{
            Foundation::{CloseHandle, HANDLE},
            Storage::FileSystem::{
                CommitTransaction, CreateFileTransactedW, CreateTransaction, FileDispositionInfo,
                RollbackTransaction, SetFileInformationByHandle, FILE_ATTRIBUTE_NORMAL,
                FILE_DISPOSITION_INFO, FILE_SHARE_READ, OPEN_EXISTING,
            },
        },
    };
    struct Transaction(HANDLE, bool);
    impl Drop for Transaction {
        fn drop(&mut self) {
            unsafe {
                if !self.1 {
                    let _ = RollbackTransaction(self.0);
                }
                let _ = CloseHandle(self.0);
            }
        }
    }
    // No enabling TxF, no nontransactional fallback. Unsupported volumes remain conflicts.
    let mut tx = Transaction(
        unsafe {
            CreateTransaction(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                0,
                0,
                10000,
                w!("ACE isolated file replacement"),
            )
        }
        .map_err(|_| "SAFE_OVERWRITE_UNAVAILABLE")?,
        false,
    );
    let open = |path: &Path, access: u32| -> Result<File, &'static str> {
        let name: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let handle = unsafe {
            CreateFileTransactedW(
                PCWSTR(name.as_ptr()),
                access,
                FILE_SHARE_READ,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
                tx.0,
                None,
                None,
            )
        }
        .map_err(|_| "SAFE_OVERWRITE_UNAVAILABLE")?;
        Ok(unsafe { File::from_raw_handle(handle.0) })
    };
    let mut input = open(
        source,
        0x80000000 | if tool == "file.copy" { 0 } else { 0x10000 },
    )?;
    let mut output = open(target, 0x80000000 | 0x40000000)?;
    if fingerprint(&mut input)? != *source_state || fingerprint(&mut output)? != *target_state {
        return Err("FILE_CONFIRMATION_STALE");
    }
    if source_state.identity.0 != target_state.identity.0 {
        return Err("SAFE_OVERWRITE_UNAVAILABLE");
    }
    // Writes remain transaction-private; the old destination is never deleted first.
    output
        .set_len(0)
        .map_err(|_| "SAFE_OVERWRITE_UNAVAILABLE")?;
    #[cfg(test)]
    if fault::selected("read") || fault::selected("write") {
        // Real partial transaction-private write, followed by a simulated I/O error.
        let mut prefix = [0; 4];
        input
            .read_exact(&mut prefix)
            .map_err(|_| "EXECUTION_FAILED")?;
        output.write_all(&prefix).map_err(|_| "EXECUTION_FAILED")?;
        fault::hit("read")?;
        fault::hit("write")?;
    }
    std::io::copy(&mut input, &mut output).map_err(|_| "SAFE_OVERWRITE_UNAVAILABLE")?;
    output.flush().map_err(|_| "SAFE_OVERWRITE_UNAVAILABLE")?;
    output
        .sync_all()
        .map_err(|_| "SAFE_OVERWRITE_UNAVAILABLE")?;
    if tool != "file.copy" {
        let deletion = FILE_DISPOSITION_INFO { DeleteFile: true };
        unsafe {
            SetFileInformationByHandle(
                HANDLE(input.as_raw_handle()),
                FileDispositionInfo,
                &deletion as *const _ as _,
                std::mem::size_of_val(&deletion) as u32,
            )
        }
        .map_err(|_| "SAFE_OVERWRITE_UNAVAILABLE")?;
        // TxF applies delete disposition when the transacted handle closes.
        // Closing inside the transaction stages deletion; rollback still restores it.
        drop(input);
    }
    #[cfg(test)]
    fault::hit("commit")?;
    #[cfg(test)]
    if abort_before_commit {
        return Err("EXECUTION_FAILED");
    }
    unsafe { CommitTransaction(tx.0) }.map_err(|_| "SAFE_OVERWRITE_UNAVAILABLE")?;
    tx.1 = true;
    Ok(())
}
#[cfg(windows)]
fn plain_streams_only(path: &Path) -> Result<(), &'static str> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{
                FindClose, FindFirstStreamW, FindNextStreamW, FindStreamInfoStandard,
                WIN32_FIND_STREAM_DATA,
            },
        },
    };
    let name: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut data = WIN32_FIND_STREAM_DATA::default();
    let handle = unsafe {
        FindFirstStreamW(
            PCWSTR(name.as_ptr()),
            FindStreamInfoStandard,
            &mut data as *mut _ as _,
            None,
        )
    }
    .map_err(|_| "SAFE_OVERWRITE_UNAVAILABLE")?;
    struct Search(HANDLE);
    impl Drop for Search {
        fn drop(&mut self) {
            unsafe {
                let _ = FindClose(self.0);
            }
        }
    }
    let _search = Search(handle);
    loop {
        let end = data
            .cStreamName
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(data.cStreamName.len());
        if String::from_utf16_lossy(&data.cStreamName[..end]) != "::$DATA" {
            return Err("SAFE_OVERWRITE_UNAVAILABLE");
        }
        if let Err(error) = unsafe { FindNextStreamW(handle, &mut data as *mut _ as _) } {
            return if error.code().0 as u32 & 0xffff == 38 {
                Ok(())
            } else {
                Err("SAFE_OVERWRITE_UNAVAILABLE")
            };
        }
    }
}
#[cfg(not(windows))]
fn plain_streams_only(_: &Path) -> Result<(), &'static str> {
    Err("SAFE_OVERWRITE_UNAVAILABLE")
}
#[cfg(not(windows))]
fn transactional_replace(
    _: &str,
    _: &Path,
    _: &Path,
    _: &Fingerprint,
    _: &Fingerprint,
    #[cfg(test)] _: bool,
) -> Result<(), &'static str> {
    Err("SAFE_OVERWRITE_UNAVAILABLE")
}

// Test-only, thread-local: never available through IPC, environment, or release builds.
#[cfg(test)]
mod fault {
    thread_local! { static STATE: std::cell::RefCell<(&'static str, bool)> = const { std::cell::RefCell::new(("", false)) }; }
    pub fn set(stage: &'static str) {
        STATE.with(|s| *s.borrow_mut() = (stage, false));
    }
    pub fn selected(stage: &str) -> bool {
        STATE.with(|s| s.borrow().0 == stage)
    }
    pub fn hit(stage: &str) -> Result<(), &'static str> {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if s.0 == stage {
                s.1 = true;
                Err("EXECUTION_FAILED")
            } else {
                Ok(())
            }
        })
    }
    pub fn reset() -> bool {
        STATE.with(|s| {
            let reached = s.borrow().1;
            *s.borrow_mut() = ("", false);
            reached
        })
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    struct Fixture {
        root: PathBuf,
        source: PathBuf,
        target: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("ace-file-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            let source = root.join("source.txt");
            let target = root.join("target.txt");
            fs::write(&source, b"source content").unwrap();
            fs::write(&target, b"old destination").unwrap();
            Self {
                root,
                source,
                target,
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if self.root.canonicalize().ok().is_some_and(|path| {
                path.parent() == std::env::temp_dir().canonicalize().ok().as_deref()
                    && path.file_name() == self.root.file_name()
            }) {
                let _ = fs::remove_dir_all(&self.root);
            }
        }
    }
    fn args() -> Value {
        serde_json::json!({"sourceDirectory":"desktop","sourcePath":"source.txt","destinationDirectory":"desktop","destinationPath":"target.txt"})
    }
    #[test]
    fn injected_failures_preserve_hashes_release_handles_and_allow_retry() {
        use std::os::windows::fs::OpenOptionsExt;
        for tool in ["file.copy", "file.move"] {
            for stage in ["read", "write", "commit"] {
                let f = Fixture::new();
                let s = FileApprovals::default();
                let hash = |p: &Path| -> Vec<u8> { Sha256::digest(fs::read(p).unwrap()).to_vec() };
                let before = (hash(&f.source), hash(&f.target));
                let id = approve(&s, &f, tool);
                fault::set(stage);
                let result = execute(&s, tool, &args(), &f.source, &f.target, Some(&id));
                let reached = fault::reset();
                assert!(reached, "unsupported volume is NOT a rollback pass");
                assert!(matches!(result, Err("EXECUTION_FAILED")));
                assert_eq!((hash(&f.source), hash(&f.target)), before);
                assert_eq!(fs::read_dir(&f.root).unwrap().count(), 2);
                for path in [&f.source, &f.target] {
                    drop(
                        OpenOptions::new()
                            .read(true)
                            .write(true)
                            .share_mode(0)
                            .open(path)
                            .unwrap(),
                    );
                }
                assert!(s.approvals.lock().unwrap().is_empty());
                assert!(matches!(
                    execute(&s, tool, &args(), &f.source, &f.target, Some(&id)),
                    Err("FILE_CONFIRMATION_STALE")
                ));
                let retry = approve(&s, &f, tool);
                assert!(matches!(
                    execute(&s, tool, &args(), &f.source, &f.target, Some(&retry)),
                    Ok(Outcome::Done)
                ));
                assert_eq!(hash(&f.target), before.0);
                assert_eq!(f.source.exists(), tool == "file.copy");
                eprintln!("INJECTED_FAILURE_REAL_TXF tool={tool} stage={stage} sha256_preserved=true handles_released=true retry=true");
            }
        }
    }
    #[test]
    fn actual_windows_lock_refusal_preserves_hashes_and_allows_retry() {
        use std::os::windows::fs::OpenOptionsExt;
        for tool in ["file.copy", "file.move"] {
            let f = Fixture::new();
            let s = FileApprovals::default();
            let hash = |p: &Path| -> Vec<u8> { Sha256::digest(fs::read(p).unwrap()).to_vec() };
            let before = (hash(&f.source), hash(&f.target));
            let id = approve(&s, &f, tool);
            let lock = OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&f.target)
                .unwrap();
            assert!(execute(&s, tool, &args(), &f.source, &f.target, Some(&id)).is_err());
            drop(lock);
            assert_eq!((hash(&f.source), hash(&f.target)), before);
            assert_eq!(fs::read_dir(&f.root).unwrap().count(), 2);
            let retry = approve(&s, &f, tool);
            assert!(matches!(
                execute(&s, tool, &args(), &f.source, &f.target, Some(&retry)),
                Ok(Outcome::Done)
            ));
            eprintln!("REAL_WINDOWS_LOCK_REFUSAL tool={tool} sha256_preserved=true retry=true (before-write refusal)");
        }
    }
    #[test]
    fn unsupported_alternate_stream_refuses_before_writes_with_hashes_preserved() {
        for tool in ["file.copy", "file.move"] {
            let f = Fixture::new();
            let stream = f.source.with_file_name("source.txt:verification-stream");
            fs::write(&stream, b"isolated test metadata").unwrap();
            let hash = |p: &Path| -> Vec<u8> { Sha256::digest(fs::read(p).unwrap()).to_vec() };
            let before = (hash(&f.source), hash(&f.target), hash(&stream));
            let s = FileApprovals::default();
            let id = approve(&s, &f, tool);
            fault::set("commit");
            let result = execute(&s, tool, &args(), &f.source, &f.target, Some(&id));
            assert!(!fault::reset());
            assert!(matches!(result, Err("SAFE_OVERWRITE_UNAVAILABLE")));
            assert_eq!((hash(&f.source), hash(&f.target), hash(&stream)), before);
            assert_eq!(fs::read_dir(&f.root).unwrap().count(), 2);
            assert!(s.approvals.lock().unwrap().is_empty());
            eprintln!(
                "REAL_UNSUPPORTED_ADS_REFUSAL tool={tool} sha256_preserved=true (not rollback)"
            );
        }
    }
    fn approve(state: &FileApprovals, f: &Fixture, tool: &str) -> String {
        match execute(state, tool, &args(), &f.source, &f.target, None).unwrap() {
            Outcome::Conflict(c) => authorize(state, &c.approval_id).unwrap(),
            _ => panic!("collision must require approval"),
        }
    }
    #[test]
    fn collision_never_changes_files_before_explicit_approval() {
        let f = Fixture::new();
        let s = FileApprovals::default();
        let Outcome::Conflict(info) =
            execute(&s, "file.copy", &args(), &f.source, &f.target, None).unwrap()
        else {
            panic!("expected confirmation");
        };
        assert!(matches!(
            execute(
                &s,
                "file.copy",
                &args(),
                &f.source,
                &f.target,
                Some(&info.approval_id)
            ),
            Err("FILE_CONFIRMATION_STALE")
        ));
        assert_eq!(fs::read(&f.target).unwrap(), b"old destination");
        assert_eq!(fs::read(&f.source).unwrap(), b"source content");
    }
    #[test]
    fn tampered_command_and_arguments_and_expired_approval_are_rejected() {
        for variant in 0..3 {
            let f = Fixture::new();
            let s = FileApprovals::default();
            let id = approve(&s, &f, "file.copy");
            let mut a = args();
            if variant == 1 {
                a["destinationPath"] = serde_json::json!("changed.txt");
            }
            if variant == 2 {
                s.approvals.lock().unwrap().get_mut(&id).unwrap().expires =
                    Instant::now() - Duration::from_secs(1);
            }
            let tool = if variant == 0 {
                "file.move"
            } else {
                "file.copy"
            };
            assert!(matches!(
                execute(&s, tool, &a, &f.source, &f.target, Some(&id)),
                Err("FILE_CONFIRMATION_STALE")
            ));
            assert_eq!(fs::read(&f.target).unwrap(), b"old destination");
        }
    }
    #[test]
    fn changed_source_or_target_aborts_without_overwriting() {
        for source in [true, false] {
            let f = Fixture::new();
            let s = FileApprovals::default();
            let id = approve(&s, &f, "file.copy");
            fs::write(
                if source { &f.source } else { &f.target },
                b"externally changed",
            )
            .unwrap();
            let result = execute(&s, "file.copy", &args(), &f.source, &f.target, Some(&id));
            assert!(matches!(
                result,
                Err("FILE_CONFIRMATION_STALE") | Err("SAFE_OVERWRITE_UNAVAILABLE")
            ));
            assert_eq!(
                fs::read(if source { &f.source } else { &f.target }).unwrap(),
                b"externally changed"
            );
            assert_eq!(
                fs::read(if source { &f.target } else { &f.source }).unwrap(),
                if source {
                    b"old destination".as_slice()
                } else {
                    b"source content".as_slice()
                }
            );
        }
    }
    #[test]
    fn overwrite_is_atomic_or_refused_on_this_volume_and_token_cannot_replay() {
        for tool in ["file.copy", "file.move", "file.rename"] {
            let f = Fixture::new();
            let s = FileApprovals::default();
            let id = approve(&s, &f, tool);
            match execute(&s, tool, &args(), &f.source, &f.target, Some(&id)) {
                Ok(Outcome::Done) => {
                    assert_eq!(fs::read(&f.target).unwrap(), b"source content");
                    assert_eq!(f.source.exists(), tool == "file.copy");
                    eprintln!("TRANSACTIONAL_OVERWRITE_SUPPORTED {tool}");
                }
                Err("SAFE_OVERWRITE_UNAVAILABLE") => {
                    assert_eq!(fs::read(&f.target).unwrap(), b"old destination");
                    assert_eq!(fs::read(&f.source).unwrap(), b"source content");
                    eprintln!("TRANSACTIONAL_OVERWRITE_UNAVAILABLE {tool}: preserved both files");
                }
                _ => panic!("unexpected replacement result"),
            };
            assert!(matches!(
                execute(&s, tool, &args(), &f.source, &f.target, Some(&id)),
                Err("FILE_CONFIRMATION_STALE")
            ));
        }
    }
    #[test]
    fn failure_after_staged_write_and_delete_intent_rolls_back_both_files() {
        for tool in ["file.copy", "file.move", "file.rename"] {
            let f = Fixture::new();
            let source_state = fingerprint(&mut read_locked(&f.source).unwrap()).unwrap();
            let target_state = fingerprint(&mut read_locked(&f.target).unwrap()).unwrap();
            let outcome = transactional_replace(
                tool,
                &f.source,
                &f.target,
                &source_state,
                &target_state,
                true,
            );
            assert!(matches!(outcome, Err("EXECUTION_FAILED")));
            assert_eq!(fs::read(&f.target).unwrap(), b"old destination");
            assert_eq!(fs::read(&f.source).unwrap(), b"source content");
            assert_eq!(fs::read_dir(&f.root).unwrap().count(), 2);
        }
    }
    #[test]
    fn no_collision_copy_is_complete_and_no_replace_rename_preserves_collisions() {
        let f = Fixture::new();
        let new = f.root.join("new.txt");
        let s = FileApprovals::default();
        assert!(matches!(
            execute(&s, "file.copy", &args(), &f.source, &new, None),
            Ok(Outcome::Done)
        ));
        assert_eq!(fs::read(new).unwrap(), b"source content");
        assert!(rename_no_replace(&f.source, &f.target).is_err());
        assert_eq!(fs::read(&f.source).unwrap(), b"source content");
        assert_eq!(fs::read(&f.target).unwrap(), b"old destination");
        assert_eq!(fs::read_dir(&f.root).unwrap().count(), 3);
    }
    #[test]
    fn copying_to_a_new_name_keeps_alternate_streams_but_overwrite_refuses_them() {
        let f = Fixture::new();
        let zone = f.source.with_file_name("source.txt:Zone.Identifier");
        fs::write(&zone, b"[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();
        let s = FileApprovals::default();
        let new = f.root.join("with-zone.txt");
        assert!(matches!(
            execute(&s, "file.copy", &args(), &f.source, &new, None),
            Ok(Outcome::Done)
        ));
        assert_eq!(
            fs::read(new.with_file_name("with-zone.txt:Zone.Identifier")).unwrap(),
            fs::read(&zone).unwrap()
        );
        let id = approve(&s, &f, "file.copy");
        assert!(matches!(
            execute(&s, "file.copy", &args(), &f.source, &f.target, Some(&id)),
            Err("SAFE_OVERWRITE_UNAVAILABLE")
        ));
        assert_eq!(fs::read(&f.target).unwrap(), b"old destination");
        assert_eq!(fs::read(&zone).unwrap(), b"[ZoneTransfer]\r\nZoneId=3\r\n");
    }
}
