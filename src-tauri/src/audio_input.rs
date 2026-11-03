//! A shared, persisted input selection for wake detection, enrollment and Orb capture.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tauri::{Emitter, Manager};

// Only use on a dedicated audio thread, before CPAL initializes COM as STA.
#[cfg(windows)]
struct AudioCom(std::marker::PhantomData<*mut ()>);
#[cfg(windows)]
impl AudioCom {
    fn initialize() -> Result<Self, String> {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }
            .map_err(|e| format!("AUDIO_COM_INIT_FAILED: {e}"))?;
        Ok(Self(std::marker::PhantomData))
    }
}
#[cfg(windows)]
impl Drop for AudioCom {
    fn drop(&mut self) {
        unsafe { windows::Win32::System::Com::CoUninitialize() };
    }
}

#[cfg(all(test, windows))]
mod com_tests {
    #[test]
    fn audio_thread_uses_mta_and_releases_its_initialization() {
        std::thread::spawn(|| {
            use windows::Win32::{
                Foundation::RPC_E_CHANGED_MODE,
                System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
            };
            {
                let _com = super::AudioCom::initialize().unwrap();
                assert_eq!(
                    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) },
                    RPC_E_CHANGED_MODE
                );
            }
            unsafe {
                CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
                CoUninitialize();
            }
        })
        .join()
        .unwrap();
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputSettings {
    pub device_id: Option<String>,
    #[serde(default)]
    pub allow_default_fallback: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDevice {
    id: String,
    name: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputStatus {
    settings: InputSettings,
    devices: Vec<InputDevice>,
    current_device_id: Option<String>,
    current_device_name: Option<String>,
}
struct Worker {
    ready: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: thread::JoinHandle<()>,
    owner: String,
    session_id: String,
}
#[derive(Default)]
pub struct AudioInputState {
    worker: Mutex<Option<Worker>>,
    operation: Mutex<()>,
}
pub fn voice_readiness(app: &tauri::AppHandle) -> Result<bool, &'static str> {
    let state = app.state::<AudioInputState>();
    let worker = state.worker.lock().map_err(|_| "MICROPHONE_UNAVAILABLE")?;
    match worker.as_ref() {
        Some(w) if w.owner == "voice" => {
            if w.thread.is_finished() || w.stop.load(Ordering::Acquire) {
                Err("MICROPHONE_UNAVAILABLE")
            } else {
                Ok(w.ready.load(Ordering::Acquire))
            }
        }
        _ => Ok(false),
    }
}

fn path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("audio-input.json"))
}
pub fn settings(app: &tauri::AppHandle) -> InputSettings {
    path(app)
        .ok()
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
#[cfg(windows)]
pub fn select_device(app: &tauri::AppHandle, host: &cpal::Host) -> Result<cpal::Device, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let preference = settings(app);
    if let Some(id) = preference.device_id {
        if let Some(device) = host
            .input_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.id().is_ok_and(|v| v.to_string() == id))
        {
            return Ok(device);
        }
        if !preference.allow_default_fallback {
            return Err("MICROPHONE_DISCONNECTED: 선택한 마이크가 연결되지 않았습니다. 설정에서 다시 선택해 주세요.".into());
        }
    }
    host.default_input_device().ok_or_else(|| "MICROPHONE_DISCONNECTED: 시스템 기본 마이크가 없습니다. 설정에서 마이크를 선택해 주세요.".into())
}

#[tauri::command]
pub fn get_audio_input_status(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<InputStatus, String> {
    if !["main", "voice"].contains(&window.label()) {
        return Err("BLOCKED".into());
    }
    #[cfg(windows)]
    {
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::default_host();
        let devices = host
            .input_devices()
            .map_err(|e| e.to_string())?
            .filter_map(|d| {
                Some(InputDevice {
                    id: d.id().ok()?.to_string(),
                    name: d.description().ok()?.name().into(),
                })
            })
            .collect();
        let current = select_device(&app, &host).ok();
        Ok(InputStatus {
            settings: settings(&app),
            devices,
            current_device_id: current
                .as_ref()
                .and_then(|d| d.id().ok())
                .map(|id| id.to_string()),
            current_device_name: current
                .and_then(|d| d.description().ok())
                .map(|d| d.name().into()),
        })
    }
    #[cfg(not(windows))]
    {
        Err("DESKTOP_REQUIRED".into())
    }
}
pub fn stop(app: &tauri::AppHandle) {
    let worker = app
        .state::<AudioInputState>()
        .worker
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some(worker) = worker {
        worker.stop.store(true, Ordering::Release);
        let _ = worker.thread.join();
    }
}
#[tauri::command]
pub fn stop_audio_input(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    if !["main", "voice"].contains(&window.label()) {
        return Err("BLOCKED".into());
    }
    let state = app.state::<AudioInputState>();
    let _operation = state.operation.lock().map_err(|_| "INPUT_BUSY")?;
    let owned = state
        .worker
        .lock()
        .map_err(|_| "INPUT_BUSY")?
        .as_ref()
        .is_some_and(|worker| worker.owner == window.label());
    if owned {
        stop(&app);
    }
    Ok(())
}
#[tauri::command]
pub fn set_audio_input(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    preference: InputSettings,
) -> Result<InputStatus, String> {
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    let state = app.state::<AudioInputState>();
    let _operation = state.operation.lock().map_err(|_| "INPUT_BUSY")?;
    if preference
        .device_id
        .as_ref()
        .is_some_and(|id| id.len() > 1024 || id.is_empty())
    {
        return Err("INVALID_ARGUMENT".into());
    }
    if preference.device_id.as_ref().is_some_and(|id| {
        settings(&app).device_id.as_ref() != Some(id)
            && !get_audio_input_status(app.clone(), window.clone())
                .is_ok_and(|s| s.devices.iter().any(|d| &d.id == id))
    }) {
        return Err("MICROPHONE_DISCONNECTED".into());
    }
    crate::wake_word::stop(&app);
    stop(&app);
    crate::voice_recording::cancel_active(&app)?;
    crate::wake_word_setup::cancel_input_for_device_change(&app)?;
    let destination = path(&app)?;
    fs::create_dir_all(destination.parent().ok_or("INPUT_STORAGE_FAILED")?)
        .map_err(|e| e.to_string())?;
    let temporary = destination.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec(&preference).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(temporary, destination).map_err(|e| e.to_string())?;
    let _ = app.emit_to("voice", "ace-audio-input-changed", ());
    crate::wake_word::restore_persisted(&app)?;
    get_audio_input_status(app.clone(), window)
}

#[tauri::command]
pub fn start_audio_input(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    session_id: String,
    monitor: bool,
) -> Result<u32, String> {
    if (monitor && window.label() != "main")
        || (!monitor && window.label() != "voice")
        || uuid::Uuid::parse_str(&session_id).is_err()
    {
        return Err("BLOCKED".into());
    }
    let state = app.state::<AudioInputState>();
    let _operation = state.operation.lock().map_err(|_| "INPUT_BUSY")?;
    if monitor && crate::voice_overlay::is_active(&app) {
        return Err("음성 명령이 끝난 뒤 음량 확인을 시작해 주세요.".into());
    }
    if let Some(worker) = state.worker.lock().map_err(|_| "INPUT_BUSY")?.as_ref() {
        if worker.owner != window.label() {
            let _ = app.emit_to(&worker.owner, "ace-audio-input-error", serde_json::json!({"sessionId":worker.session_id,"message":"음성 입력이 시작되어 음량 확인을 종료했습니다."}));
        }
    }
    stop(&app);
    #[cfg(windows)]
    {
        let activity = crate::activity::begin().map_err(str::to_owned)?;
        let stop_flag = Arc::new(AtomicBool::new(false));
        let ready_flag = Arc::new(AtomicBool::new(false));
        let stopping = stop_flag.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let worker_app = app.clone();
        let label = window.label().to_owned();
        let owner = label.clone();
        let worker_session = session_id.clone();
        let worker = thread::spawn(move || {
            let _activity = activity;
            let result = capture(
                &worker_app,
                &label,
                &session_id,
                monitor,
                stopping,
                &ready_tx,
            );
            if let Err(error) = result {
                let _ = ready_tx.try_send(Err(error.clone()));
                let _ = worker_app.emit_to(
                    &label,
                    "ace-audio-input-error",
                    serde_json::json!({"sessionId":session_id,"message":error}),
                );
            }
        });
        *state.worker.lock().map_err(|_| "INPUT_BUSY")? = Some(Worker {
            ready: ready_flag.clone(),
            stop: stop_flag,
            thread: worker,
            owner,
            session_id: worker_session,
        });
        match ready_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(rate)) => {
                ready_flag.store(true, Ordering::Release);
                Ok(rate)
            }
            Ok(Err(e)) => {
                stop(&app);
                Err(e)
            }
            Err(_) => {
                stop(&app);
                Err("INPUT_START_TIMEOUT".into())
            }
        }
    }
    #[cfg(not(windows))]
    {
        Err("DESKTOP_REQUIRED".into())
    }
}

#[cfg(windows)]
fn capture(
    app: &tauri::AppHandle,
    label: &str,
    session: &str,
    monitor: bool,
    stop: Arc<AtomicBool>,
    ready: &std::sync::mpsc::SyncSender<Result<u32, String>>,
) -> Result<(), String> {
    use cpal::traits::{DeviceTrait, StreamTrait};
    let _com = AudioCom::initialize()?;
    let host = cpal::default_host();
    let device = select_device(app, &host)?;
    let device_id = device.id().map_err(|e| e.to_string())?.to_string();
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let rate = supported.sample_rate();
    let channels = supported.channels();
    let config: cpal::StreamConfig = supported.into();
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(32);
    let lost = Arc::new(AtomicBool::new(false));
    let callback_lost = lost.clone();
    let (error_tx, errors) = std::sync::mpsc::channel();
    let on_error = move |e: cpal::Error| {
        if e.kind() != cpal::ErrorKind::Xrun {
            let _ = error_tx.send(());
        }
    };
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _| {
                if tx
                    .try_send(crate::wake_word::downmix_f32(data, channels))
                    .is_err()
                {
                    callback_lost.store(true, Ordering::Release);
                }
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _| {
                if tx
                    .try_send(
                        crate::wake_word::downmix_i16(data, channels)
                            .into_iter()
                            .map(|v| v as f32 / 32768.0)
                            .collect(),
                    )
                    .is_err()
                {
                    callback_lost.store(true, Ordering::Release);
                }
            },
            on_error,
            None,
        ),
        _ => return Err("MICROPHONE_UNSUPPORTED_FORMAT".into()),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    let _ = ready.send(Ok(rate));
    let mut last_check = std::time::Instant::now();
    while !stop.load(Ordering::Acquire) {
        if errors.try_recv().is_ok() {
            return Err("마이크 연결이 끊겼습니다. 설정에서 마이크를 다시 선택해 주세요.".into());
        }
        if lost.load(Ordering::Acquire) {
            return Err("음성 입력 일부를 받지 못했습니다. 다시 말씀해 주세요.".into());
        }
        if last_check.elapsed() > Duration::from_secs(1) {
            use cpal::traits::HostTrait;
            if !host
                .input_devices()
                .map_err(|e| e.to_string())?
                .any(|d| d.id().is_ok_and(|id| id.to_string() == device_id))
            {
                return Err("마이크 연결이 끊겼습니다. 설정에서 다시 선택해 주세요.".into());
            }
            last_check = std::time::Instant::now();
        }
        if let Ok(samples) = rx.recv_timeout(Duration::from_millis(40)) {
            let rms =
                (samples.iter().map(|v| v * v).sum::<f32>() / samples.len().max(1) as f32).sqrt();
            app.emit_to(label, "ace-audio-input", serde_json::json!({"sessionId":session,"sampleRate":rate,"rms":rms,"samples":if monitor {None} else {Some(samples)}})).map_err(|e| e.to_string())?;
        }
    }
    drop(stream);
    Ok(())
}
