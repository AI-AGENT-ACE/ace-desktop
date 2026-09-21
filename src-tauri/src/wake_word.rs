use serde::Serialize;
use std::{
    sync::{mpsc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};

use crate::voice_overlay;

const TRIGGER_COOLDOWN: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WakeWordState {
    Disabled,
    Starting,
    Listening,
    Triggered,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeWordStatus {
    pub state: WakeWordState,
    pub enabled: bool,
    pub error: Option<String>,
}

struct ListenerControl {
    stop: mpsc::Sender<()>,
    thread: JoinHandle<()>,
}

pub struct WakeWordRuntime {
    status: Mutex<WakeWordStatus>,
    listener: Mutex<Option<ListenerControl>>,
    last_trigger: Mutex<Option<Instant>>,
}

impl Default for WakeWordRuntime {
    fn default() -> Self {
        Self {
            status: Mutex::new(WakeWordStatus {
                state: WakeWordState::Disabled,
                enabled: false,
                error: None,
            }),
            listener: Mutex::new(None),
            last_trigger: Mutex::new(None),
        }
    }
}

impl WakeWordRuntime {
    pub fn status(&self) -> WakeWordStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn update(&self, app: &AppHandle, state: WakeWordState, enabled: bool, error: Option<String>) {
        let status = WakeWordStatus {
            state,
            enabled,
            error,
        };
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = status.clone();
        let _ = app.emit_to("main", "ace-wake-word-status", &status);
        let _ = app.emit_to("main", "ace-wake-word-changed", enabled);
        crate::tray::sync_wake_word_item(app, &status);
    }

    fn should_trigger(&self, now: Instant) -> bool {
        let mut last = self.last_trigger.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|value| now.duration_since(value) < TRIGGER_COOLDOWN) {
            return false;
        }
        *last = Some(now);
        true
    }
}

pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<WakeWordStatus, String> {
    if enabled {
        start(app)?;
    } else {
        stop(app);
    }
    Ok(app.state::<WakeWordRuntime>().status())
}

pub fn stop(app: &AppHandle) {
    let runtime = app.state::<WakeWordRuntime>();
    let control = runtime
        .listener
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some(control) = control {
        let _ = control.stop.send(());
        let _ = control.thread.join();
    }
    runtime.update(app, WakeWordState::Disabled, false, None);
}

fn start(app: &AppHandle) -> Result<(), String> {
    let runtime = app.state::<WakeWordRuntime>();
    if runtime
        .listener
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some()
    {
        return Ok(());
    }
    runtime.update(app, WakeWordState::Starting, true, None);
    let (stop_tx, stop_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let worker_app = app.clone();
    let worker = thread::Builder::new()
        .name("ace-wake-word".into())
        .spawn(move || run_listener(worker_app, stop_rx, ready_tx))
        .map_err(|error| error.to_string())?;
    match ready_rx.recv_timeout(Duration::from_secs(12)) {
        Ok(Ok(())) => {
            *runtime.listener.lock().unwrap_or_else(|e| e.into_inner()) = Some(ListenerControl {
                stop: stop_tx,
                thread: worker,
            });
            runtime.update(app, WakeWordState::Listening, true, None);
            Ok(())
        }
        Ok(Err(error)) => {
            let _ = worker.join();
            runtime.update(app, WakeWordState::Error, false, Some(error.clone()));
            Err(error)
        }
        Err(_) => {
            let _ = stop_tx.send(());
            let error = "Wake Word 엔진 시작 시간이 초과되었습니다.".to_owned();
            runtime.update(app, WakeWordState::Error, false, Some(error.clone()));
            Err(error)
        }
    }
}

fn handle_detection(app: &AppHandle, text: &str, confidence_accepted: bool) {
    if !is_wake_phrase(text, confidence_accepted) {
        return;
    }
    let runtime = app.state::<WakeWordRuntime>();
    if !runtime.should_trigger(Instant::now()) || voice_overlay::is_active(app) {
        return;
    }
    runtime.update(app, WakeWordState::Triggered, true, None);
    if let Err(error) = voice_overlay::activate(app) {
        runtime.update(app, WakeWordState::Error, true, Some(error));
    } else {
        runtime.update(app, WakeWordState::Listening, true, None);
    }
}

fn is_wake_phrase(text: &str, confidence_accepted: bool) -> bool {
    confidence_accepted && text.trim().eq_ignore_ascii_case("ace")
}

#[cfg(windows)]
fn run_listener(
    app: AppHandle,
    stop_rx: mpsc::Receiver<()>,
    ready: mpsc::SyncSender<Result<(), String>>,
) {
    use windows::{
        core::{Interface, HSTRING},
        Foundation::TypedEventHandler,
        Media::SpeechRecognition::{
            ISpeechRecognitionConstraint, SpeechContinuousRecognitionResultGeneratedEventArgs,
            SpeechContinuousRecognitionSession, SpeechRecognitionConfidence,
            SpeechRecognitionListConstraint, SpeechRecognitionResultStatus, SpeechRecognizer,
        },
        Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
    };
    use windows_collections::IIterable;

    struct WinRtApartment;
    impl Drop for WinRtApartment {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }

    let setup = (|| -> windows::core::Result<_> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
        let apartment = WinRtApartment;
        let recognizer = SpeechRecognizer::new()?;
        let commands: IIterable<HSTRING> = vec![HSTRING::from("ACE")].into();
        let constraint = SpeechRecognitionListConstraint::Create(&commands)?;
        recognizer
            .Constraints()?
            .Append(&constraint.cast::<ISpeechRecognitionConstraint>()?)?;
        let compiled = recognizer.CompileConstraintsAsync()?.get()?;
        if compiled.Status()? != SpeechRecognitionResultStatus::Success {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x80004005u32 as i32),
                "ACE Wake Word 문법을 초기화하지 못했습니다.",
            ));
        }
        let session = recognizer.ContinuousRecognitionSession()?;
        let callback_app = app.clone();
        let token = session.ResultGenerated(&TypedEventHandler::<
            SpeechContinuousRecognitionSession,
            SpeechContinuousRecognitionResultGeneratedEventArgs,
        >::new(move |_, args| {
            let result = args.ok()?.Result()?;
            let accepted = result.Status()? == SpeechRecognitionResultStatus::Success
                && matches!(
                    result.Confidence()?,
                    SpeechRecognitionConfidence::High | SpeechRecognitionConfidence::Medium
                );
            handle_detection(&callback_app, &result.Text()?.to_string(), accepted);
            Ok(())
        }))?;
        session.StartAsync()?.get()?;
        Ok((apartment, recognizer, session, token))
    })();

    match setup {
        Ok((_apartment, recognizer, session, token)) => {
            let _ = ready.send(Ok(()));
            let _ = stop_rx.recv();
            let _ = session.RemoveResultGenerated(token);
            let _ = session.CancelAsync().and_then(|operation| operation.get());
            let _ = recognizer.Close();
        }
        Err(error) => {
            let message = microphone_error(&error);
            let _ = ready.send(Err(message));
        }
    }
}

#[cfg(not(windows))]
fn run_listener(
    _app: AppHandle,
    _stop_rx: mpsc::Receiver<()>,
    ready: mpsc::SyncSender<Result<(), String>>,
) {
    let _ = ready.send(Err("Wake Word는 현재 Windows에서 지원됩니다.".into()));
}

#[cfg(windows)]
fn microphone_error(error: &windows::core::Error) -> String {
    let code = error.code().0 as u32;
    match code {
        0x80070005 => "마이크 권한이 거부되었습니다. Windows 개인정보 설정에서 ACE의 마이크 접근을 허용해 주세요.".into(),
        0x80070490 | 0x8007000f => "사용 가능한 마이크 장치를 찾지 못했습니다.".into(),
        0x800700aa | 0x8889000a => "마이크가 다른 프로그램에서 사용 중이거나 스트림을 만들 수 없습니다.".into(),
        _ => format!("Wake Word 엔진 초기화에 실패했습니다. ({code:#010x})"),
    }
}

#[tauri::command]
pub fn get_wake_word_status(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<WakeWordStatus, String> {
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    Ok(app.state::<WakeWordRuntime>().status())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_detection_is_rejected_during_cooldown() {
        let runtime = WakeWordRuntime::default();
        let now = Instant::now();
        assert!(runtime.should_trigger(now));
        assert!(!runtime.should_trigger(now + Duration::from_secs(1)));
        assert!(runtime.should_trigger(now + TRIGGER_COOLDOWN));
    }

    #[test]
    fn only_ace_with_accepted_confidence_triggers() {
        assert!(is_wake_phrase("ACE", true));
        assert!(is_wake_phrase(" ace ", true));
        assert!(!is_wake_phrase("ACE", false));
        assert!(!is_wake_phrase("HEY ACE", true));
    }
}
