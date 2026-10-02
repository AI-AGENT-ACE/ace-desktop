use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};

use crate::voice_overlay;

const TRIGGER_COOLDOWN: Duration = Duration::from_secs(3);
#[cfg(windows)]
static NEXT_LISTENER_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(windows)]
const WAKE_MIC_DEVICE_NOT_FOUND: &str = "WAKE_MIC_DEVICE_NOT_FOUND";
#[cfg(windows)]
const WAKE_MIC_ENUMERATION_FAILED: &str = "WAKE_MIC_ENUMERATION_FAILED";
#[cfg(windows)]
const WAKE_MIC_CONFIG_FAILED: &str = "WAKE_MIC_CONFIG_FAILED";
#[cfg(windows)]
const WAKE_MIC_UNSUPPORTED_FORMAT: &str = "WAKE_MIC_UNSUPPORTED_FORMAT";
#[cfg(windows)]
const WAKE_MIC_STREAM_BUILD_FAILED: &str = "WAKE_MIC_STREAM_BUILD_FAILED";
#[cfg(windows)]
const WAKE_MIC_STREAM_START_FAILED: &str = "WAKE_MIC_STREAM_START_FAILED";
#[cfg(windows)]
const WAKE_MIC_XRUN: &str = "WAKE_MIC_XRUN";
const WAKE_MIC_PERMISSION_DENIED: &str = "WAKE_MIC_PERMISSION_DENIED";
const WAKE_ENGINE_RUNTIME_FAILED: &str = "WAKE_ENGINE_RUNTIME_FAILED";
#[cfg(windows)]
const WAKE_ENGINE_INIT_FAILED: &str = "WAKE_ENGINE_INIT_FAILED";
#[cfg(windows)]
const WAKE_ENGINE_MODEL_LOAD_FAILED: &str = "WAKE_ENGINE_MODEL_LOAD_FAILED";

#[cfg(windows)]
fn wake_error(code: &str, message: &str, detail: impl std::fmt::Display) -> String {
    let error = format!("{code}: {message} ({detail})");
    eprintln!("[ACE Wake Word] {error}");
    error
}

#[cfg(windows)]
pub(crate) fn wake_log(message: impl std::fmt::Display) {
    if cfg!(debug_assertions) {
        eprintln!("[ACE Wake Word] {message}");
    }
}

fn wake_error_code(error: &str) -> &str {
    error
        .split(':')
        .next()
        .unwrap_or(WAKE_ENGINE_RUNTIME_FAILED)
}

fn wake_user_message(code: &str) -> String {
    match code {
        WAKE_MIC_PERMISSION_DENIED => "Windows 설정에서 데스크톱 앱의 마이크 접근을 허용해 주세요.",
        WAKE_MIC_DEVICE_NOT_FOUND => "Wake Word에 사용할 음성 입력 장치를 찾지 못했습니다.",
        WAKE_MIC_ENUMERATION_FAILED
        | WAKE_MIC_CONFIG_FAILED
        | WAKE_MIC_UNSUPPORTED_FORMAT
        | WAKE_MIC_STREAM_BUILD_FAILED
        | WAKE_MIC_STREAM_START_FAILED => "Wake Word 마이크 초기화에 실패했습니다.",
        WAKE_ENGINE_INIT_FAILED | WAKE_ENGINE_MODEL_LOAD_FAILED | WAKE_ENGINE_RUNTIME_FAILED => {
            "Wake Word 엔진을 시작하지 못했습니다."
        }
        _ => "Wake Word를 시작하지 못했습니다.",
    }
    .to_owned()
}

fn retryable_error(code: &str) -> bool {
    matches!(
        code,
        WAKE_MIC_DEVICE_NOT_FOUND
            | WAKE_MIC_ENUMERATION_FAILED
            | WAKE_MIC_CONFIG_FAILED
            | WAKE_MIC_STREAM_BUILD_FAILED
            | WAKE_MIC_STREAM_START_FAILED
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WakeWordState {
    Disabled,
    Starting,
    Listening,
    Triggered,
    Paused,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeWordStatus {
    pub state: WakeWordState,
    pub enabled: bool,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

struct ListenerControl {
    id: u64,
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
                error_code: None,
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

    fn update(
        &self,
        app: &AppHandle,
        state: WakeWordState,
        enabled: bool,
        error: Option<String>,
        error_code: Option<String>,
    ) {
        let status = WakeWordStatus {
            state,
            enabled,
            error,
            error_code,
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
    crate::wake_word_setup::set_persisted_enabled(app, enabled)?;
    if enabled {
        start(app)?;
    } else {
        stop(app);
    }
    Ok(app.state::<WakeWordRuntime>().status())
}

pub fn restore_persisted(app: &AppHandle) -> Result<(), String> {
    let settings = crate::wake_word_setup::load_settings(app);
    if settings.enabled && settings.setup_completed && settings.reference_exists {
        start(app)
    } else {
        Ok(())
    }
}

pub fn stop(app: &AppHandle) {
    stop_with_reason(app, "stop requested");
}

pub(crate) fn stop_for_setup(app: &AppHandle) {
    stop_with_reason(app, "onboarding/setup started");
}

fn stop_with_reason(app: &AppHandle, reason: &str) {
    let runtime = app.state::<WakeWordRuntime>();
    let control = runtime
        .listener
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some(control) = control {
        wake_log(format!(
            "listener stop requested: instance_id={}, reason={reason}",
            control.id
        ));
        let _ = control.stop.send(());
        let _ = control.thread.join();
        wake_log(format!("listener stopped: instance_id={}", control.id));
    }
    runtime.update(app, WakeWordState::Disabled, false, None, None);
}

fn start(app: &AppHandle) -> Result<(), String> {
    let settings = crate::wake_word_setup::load_settings(app);
    if !(settings.enabled && settings.setup_completed && settings.reference_exists) {
        return Err(
            "WAKE_SETUP_REQUIRED: 음성 호출을 사용하려면 먼저 목소리를 등록해주세요.".to_owned(),
        );
    }
    let runtime = app.state::<WakeWordRuntime>();
    let finished = {
        let mut listener = runtime.listener.lock().unwrap_or_else(|e| e.into_inner());
        if listener
            .as_ref()
            .is_some_and(|control| control.thread.is_finished())
        {
            listener.take()
        } else {
            None
        }
    };
    if let Some(control) = finished {
        let _ = control.thread.join();
    }
    if let Some(id) = runtime
        .listener
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|control| control.id)
    {
        wake_log(format!(
            "listener start skipped: instance_id={id}, reason=already_running"
        ));
        return Ok(());
    }
    runtime.update(app, WakeWordState::Starting, true, None, None);
    let retry_delays = [
        Duration::ZERO,
        Duration::from_millis(400),
        Duration::from_millis(900),
    ];
    let mut last_error = String::new();
    for (index, delay) in retry_delays.into_iter().enumerate() {
        if !delay.is_zero() {
            thread::sleep(delay);
        }
        let attempt = index + 1;
        wake_log(format!("listener initialization attempt: {attempt}/3"));
        let listener_id = NEXT_LISTENER_ID.fetch_add(1, Ordering::Relaxed);
        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker_app = app.clone();
        let worker = match thread::Builder::new()
            .name(format!("ace-wake-word-{listener_id}"))
            .spawn(move || run_listener(worker_app, listener_id, stop_rx, ready_tx))
        {
            Ok(worker) => worker,
            Err(error) => {
                last_error = format!(
                    "WAKE_ENGINE_INIT_FAILED: Wake Word 작업 스레드를 시작하지 못했습니다. ({error})"
                );
                break;
            }
        };
        match ready_rx.recv_timeout(Duration::from_secs(12)) {
            Ok(Ok(())) => {
                *runtime.listener.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(ListenerControl {
                        id: listener_id,
                        stop: stop_tx,
                        thread: worker,
                    });
                runtime.update(app, WakeWordState::Listening, true, None, None);
                return Ok(());
            }
            Ok(Err(error)) => {
                let _ = worker.join();
                let code = wake_error_code(&error).to_owned();
                wake_log(format!(
                    "listener initialization failed: attempt={attempt}, errorCode={code}"
                ));
                last_error = error;
                if !retryable_error(&code) {
                    break;
                }
            }
            Err(_) => {
                let _ = stop_tx.send(());
                drop(worker);
                last_error = "WAKE_ENGINE_INIT_FAILED: Wake Word 엔진 시작 시간이 초과되었습니다."
                    .to_owned();
                break;
            }
        }
    }
    let code = wake_error_code(&last_error).to_owned();
    let message = wake_user_message(&code);
    runtime.update(
        app,
        WakeWordState::Error,
        true,
        Some(message.clone()),
        Some(code),
    );
    Err(message)
}

pub fn retry(app: &AppHandle) -> Result<WakeWordStatus, String> {
    start(app)?;
    Ok(app.state::<WakeWordRuntime>().status())
}

fn handle_detection(app: &AppHandle, text: &str, confidence_accepted: bool) -> bool {
    if !is_wake_phrase(text, confidence_accepted) {
        wake_log("detection rejected: reason=phrase_or_confidence");
        return false;
    }
    let runtime = app.state::<WakeWordRuntime>();
    if voice_overlay::is_active(app) {
        wake_log("detection rejected: reason=voice_overlay_active");
        return false;
    }
    if !runtime.should_trigger(Instant::now()) {
        wake_log("detection rejected: reason=cooldown");
        return false;
    }
    wake_log("detection accepted: forwarding to voice orb");
    runtime.update(app, WakeWordState::Triggered, true, None, None);
    if let Err(error) = voice_overlay::activate(app) {
        let code = wake_error_code(&error).to_owned();
        runtime.update(
            app,
            WakeWordState::Error,
            true,
            Some(wake_user_message(&code)),
            Some(code),
        );
        false
    } else {
        wake_log("voice orb activation: success");
        true
    }
}

fn is_wake_phrase(text: &str, confidence_accepted: bool) -> bool {
    confidence_accepted && text.trim().eq_ignore_ascii_case("ace")
}

#[cfg(windows)]
enum ListenerEvent {
    FatalStreamError(cpal::ErrorKind, String),
}

#[cfg(windows)]
enum AudioChunk {
    I16(Vec<i16>),
    F32(Vec<f32>),
}

#[cfg(windows)]
#[derive(Default)]
struct AudioMetrics {
    callback_count: AtomicU64,
    callback_ns: AtomicU64,
    worker_count: AtomicU64,
    worker_ns: AtomicU64,
    queue_depth: AtomicUsize,
    dropped_chunks: AtomicU64,
    xrun_count: AtomicU64,
    first_xrun_ms: AtomicU64,
}

#[cfg(windows)]
impl AudioMetrics {
    fn new() -> Self {
        Self {
            first_xrun_ms: AtomicU64::new(u64::MAX),
            ..Self::default()
        }
    }
}

#[cfg(windows)]
fn enqueue_audio_chunk(
    chunks: &mpsc::SyncSender<AudioChunk>,
    metrics: &AudioMetrics,
    chunk: AudioChunk,
) {
    metrics.queue_depth.fetch_add(1, Ordering::Relaxed);
    match chunks.try_send(chunk) {
        Ok(()) => {}
        Err(mpsc::TrySendError::Full(_)) => {
            metrics.queue_depth.fetch_sub(1, Ordering::Relaxed);
            metrics.dropped_chunks.fetch_add(1, Ordering::Relaxed);
        }
        Err(mpsc::TrySendError::Disconnected(_)) => {
            metrics.queue_depth.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

#[cfg(windows)]
fn process_kws_samples<T: crate::wake_kws::InputSample>(
    spotter: &mut rustpotter::Rustpotter,
    pending: &mut Vec<T>,
    input: &[T],
    diagnostics: &mut KwsScoreDiagnostics,
    processed_frames: &mut u64,
) -> bool {
    let frame_size = spotter.get_samples_per_frame();
    let mut detected = false;
    crate::wake_kws::feed(pending, input, frame_size, |frame| {
        #[cfg(debug_assertions)]
        crate::wake_diagnostics::before_frame(&frame);
        let detection = spotter.process_samples(frame);
        #[cfg(debug_assertions)]
        crate::wake_diagnostics::after_frame(spotter, detection.is_some());
        *processed_frames = processed_frames.saturating_add(1);
        diagnostics.observe(spotter.get_partial_detection(), detection.as_ref());
        if let Some(detection) = detection {
            wake_log(format!(
                "local KWS detection: name={}, score={:.3}, avg_score={:.3}, partial_count={}",
                detection.name, detection.score, detection.avg_score, detection.counter
            ));
            detected = true;
        }
    });
    detected
}

#[cfg(windows)]
#[derive(Debug, Clone, PartialEq)]
struct KwsScoreSnapshot {
    name: String,
    score: f32,
    avg_score: f32,
    reference_scores: Vec<(String, f32)>,
    partial_count: usize,
    gain: f32,
}

#[cfg(windows)]
impl From<&rustpotter::RustpotterDetection> for KwsScoreSnapshot {
    fn from(detection: &rustpotter::RustpotterDetection) -> Self {
        let mut reference_scores = detection
            .scores
            .iter()
            .map(|(name, score)| (name.clone(), *score))
            .collect::<Vec<_>>();
        reference_scores.sort_by(|left, right| left.0.cmp(&right.0));
        Self {
            name: detection.name.clone(),
            score: detection.score,
            avg_score: detection.avg_score,
            reference_scores,
            partial_count: detection.counter,
            gain: detection.gain,
        }
    }
}

#[cfg(windows)]
#[derive(Default)]
struct KwsScoreDiagnostics {
    last_partial: Option<KwsScoreSnapshot>,
    candidate_peak_score: f32,
    candidate_peak_avg_score: f32,
    candidate_peak_reference_scores: Vec<(String, f32)>,
}

#[cfg(windows)]
#[derive(Default)]
struct SignalLevels {
    sum_squares: f64,
    peak: f32,
    samples: u64,
}

#[cfg(windows)]
impl SignalLevels {
    fn observe_f32(&mut self, samples: &[f32]) {
        for sample in samples {
            let normalized = sample.clamp(-1.0, 1.0);
            self.sum_squares += f64::from(normalized).powi(2);
            self.peak = self.peak.max(normalized.abs());
        }
        self.samples = self.samples.saturating_add(samples.len() as u64);
    }

    fn observe_i16(&mut self, samples: &[i16]) {
        for sample in samples {
            let normalized = f32::from(*sample) / i16::MAX as f32;
            self.sum_squares += f64::from(normalized).powi(2);
            self.peak = self.peak.max(normalized.abs());
        }
        self.samples = self.samples.saturating_add(samples.len() as u64);
    }

    fn take(&mut self) -> (f64, f32, u64) {
        let result = (
            (self.sum_squares / self.samples.max(1) as f64).sqrt(),
            self.peak,
            self.samples,
        );
        *self = Self::default();
        result
    }
}

#[cfg(windows)]
impl KwsScoreDiagnostics {
    fn observe(
        &mut self,
        partial: Option<&rustpotter::RustpotterDetection>,
        final_detection: Option<&rustpotter::RustpotterDetection>,
    ) {
        if !cfg!(debug_assertions) {
            return;
        }

        if let Some(detection) = final_detection {
            let snapshot = KwsScoreSnapshot::from(detection);
            self.update_peaks(&snapshot);
            self.log_snapshot("final", &snapshot);
            self.log_candidate_summary("accepted");
            self.reset_candidate();
            return;
        }

        match partial.map(KwsScoreSnapshot::from) {
            Some(snapshot) => {
                self.update_peaks(&snapshot);
                if self.last_partial.as_ref() != Some(&snapshot) {
                    self.log_snapshot("partial", &snapshot);
                    self.last_partial = Some(snapshot);
                }
            }
            None if self.last_partial.is_some() => {
                self.log_candidate_summary("rejected");
                self.reset_candidate();
            }
            None => {}
        }
    }

    fn update_peaks(&mut self, snapshot: &KwsScoreSnapshot) {
        self.candidate_peak_score = self.candidate_peak_score.max(snapshot.score);
        self.candidate_peak_avg_score = self.candidate_peak_avg_score.max(snapshot.avg_score);
        for (name, score) in &snapshot.reference_scores {
            if let Some((_, peak)) = self
                .candidate_peak_reference_scores
                .iter_mut()
                .find(|(reference, _)| reference == name)
            {
                *peak = peak.max(*score);
            } else {
                self.candidate_peak_reference_scores
                    .push((name.clone(), *score));
            }
        }
        self.candidate_peak_reference_scores
            .sort_by(|left, right| left.0.cmp(&right.0));
    }

    fn log_snapshot(&self, kind: &str, snapshot: &KwsScoreSnapshot) {
        wake_log(format!(
            "KWS score diagnostic: kind={kind}, name={}, score={:.4}, avg_score={:.4}, partial_count={}, gain={:.4}, reference_scores={}",
            snapshot.name,
            snapshot.score,
            snapshot.avg_score,
            snapshot.partial_count,
            snapshot.gain,
            format_reference_scores(&snapshot.reference_scores),
        ));
    }

    fn log_candidate_summary(&self, outcome: &str) {
        wake_log(format!(
            "KWS candidate summary: outcome={outcome}, peak_score={:.4}, peak_avg_score={:.4}, peak_reference_scores={}",
            self.candidate_peak_score,
            self.candidate_peak_avg_score,
            format_reference_scores(&self.candidate_peak_reference_scores),
        ));
    }

    fn reset_candidate(&mut self) {
        self.last_partial = None;
        self.candidate_peak_score = 0.0;
        self.candidate_peak_avg_score = 0.0;
        self.candidate_peak_reference_scores.clear();
    }
}

#[cfg(windows)]
fn format_reference_scores(scores: &[(String, f32)]) -> String {
    scores
        .iter()
        .map(|(name, score)| format!("{name}:{score:.4}"))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(windows)]
fn downmix_i16(samples: &[i16], channels: u16) -> Vec<i16> {
    let channels = channels.max(1) as usize;
    if channels == 1 {
        return samples.to_vec();
    }
    samples
        .chunks_exact(channels)
        .map(|frame| {
            let sum = frame.iter().map(|sample| i64::from(*sample)).sum::<i64>();
            (sum / channels as i64).clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
        })
        .collect()
}

#[cfg(windows)]
fn downmix_f32(samples: &[f32], channels: u16) -> Vec<f32> {
    let channels = channels.max(1) as usize;
    if channels == 1 {
        return samples.to_vec();
    }
    samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

#[cfg(windows)]
fn handle_cpal_runtime_error(
    error: cpal::Error,
    metrics: &AudioMetrics,
    events: &mpsc::SyncSender<ListenerEvent>,
    stream_started: Instant,
) {
    match error.kind() {
        cpal::ErrorKind::Xrun => {
            metrics.xrun_count.fetch_add(1, Ordering::Relaxed);
            let elapsed = stream_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            let _ = metrics.first_xrun_ms.compare_exchange(
                u64::MAX,
                elapsed,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
        cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::RealtimeDenied => {}
        kind => {
            let _ = events.try_send(ListenerEvent::FatalStreamError(kind, error.to_string()));
        }
    }
}

#[cfg(windows)]
fn initialization_error_code(error: &cpal::Error, fallback: &'static str) -> &'static str {
    match error.kind() {
        cpal::ErrorKind::PermissionDenied => WAKE_MIC_PERMISSION_DENIED,
        cpal::ErrorKind::DeviceNotAvailable => WAKE_MIC_DEVICE_NOT_FOUND,
        _ => fallback,
    }
}

#[cfg(windows)]
fn run_listener(
    app: AppHandle,
    listener_id: u64,
    stop_rx: mpsc::Receiver<()>,
    ready: mpsc::SyncSender<Result<(), String>>,
) {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    wake_log(format!("listener started: instance_id={listener_id}"));

    let setup = (|| -> Result<_, String> {
        let host = cpal::default_host();
        wake_log(format!(
            "cpal host: id={:?}, name={}",
            host.id(),
            host.id().name()
        ));
        let devices = host
            .input_devices()
            .map_err(|error| {
                wake_error(
                    WAKE_MIC_ENUMERATION_FAILED,
                    "마이크 장치 목록을 읽지 못했습니다.",
                    error,
                )
            })?
            .collect::<Vec<_>>();
        wake_log(format!("input device count: {}", devices.len()));
        for (index, device) in devices.iter().enumerate() {
            match device.description() {
                Ok(description) => {
                    wake_log(format!("input device[{index}]: {}", description.name()))
                }
                Err(error) => wake_log(format!("input device[{index}] name failed: {error}")),
            }
        }
        let default_device = host.default_input_device();
        wake_log(format!(
            "default input device: {}",
            if default_device.is_some() {
                "available"
            } else {
                "none"
            }
        ));
        let device = if let Some(device) = default_device {
            device
        } else {
            let mut usable = devices
                .into_iter()
                .filter(|candidate| {
                    candidate.default_input_config().is_ok_and(|config| {
                        matches!(
                            config.sample_format(),
                            cpal::SampleFormat::I16 | cpal::SampleFormat::F32
                        )
                    })
                })
                .collect::<Vec<_>>();
            match usable.len() {
                0 => {
                    return Err(wake_error(
                        WAKE_MIC_DEVICE_NOT_FOUND,
                        "사용 가능한 마이크 장치를 찾지 못했습니다.",
                        "enumeration returned no compatible input device",
                    ));
                }
                1 => {
                    wake_log("default device unavailable; using the only compatible input device");
                    usable.remove(0)
                }
                count => {
                    return Err(wake_error(
                        WAKE_MIC_CONFIG_FAILED,
                        "기본 마이크가 없고 사용 가능한 입력 장치가 여러 개입니다.",
                        format!("{count} compatible candidates; device selection required"),
                    ));
                }
            }
        };
        let device_name = device
            .description()
            .map(|description| description.name().to_owned())
            .map_err(|error| {
                wake_error(
                    WAKE_MIC_ENUMERATION_FAILED,
                    "선택된 마이크 이름을 읽지 못했습니다.",
                    error,
                )
            })?;
        wake_log(format!("selected input device: {device_name}"));
        let supported = device.default_input_config().map_err(|error| {
            wake_error(
                WAKE_MIC_CONFIG_FAILED,
                "기본 마이크 입력 설정을 읽지 못했습니다.",
                error,
            )
        })?;
        wake_log(format!("default input config: {supported:?}"));
        wake_log(format!("sample rate: {} Hz", supported.sample_rate()));
        wake_log(format!("channel count: {}", supported.channels()));
        wake_log(format!("sample format: {:?}", supported.sample_format()));

        let sample_format = match supported.sample_format() {
            cpal::SampleFormat::I16 => rustpotter::SampleFormat::I16,
            cpal::SampleFormat::F32 => rustpotter::SampleFormat::F32,
            format => {
                return Err(wake_error(
                    WAKE_MIC_UNSUPPORTED_FORMAT,
                    "로컬 KWS가 지원하지 않는 마이크 샘플 형식입니다.",
                    format!("{format:?}"),
                ));
            }
        };
        let kws_config = crate::wake_kws::config(supported.sample_rate() as usize, sample_format);
        // Rustpotter otherwise keeps only the first input channel. Microphone Array devices
        // commonly expose stereo input, so the worker explicitly downmixes every frame to mono.
        let input_channels = supported.channels();
        wake_log(format!(
            "KWS config: instance_id={listener_id}, sample_rate={}, input_channels={}, engine_channels=1, sample_format={:?}, threshold={:.2}, avg_threshold={:.2}, min_scores={}, eager={}",
            supported.sample_rate(),
            input_channels,
            supported.sample_format(),
            kws_config.detector.threshold,
            kws_config.detector.avg_threshold,
            kws_config.detector.min_scores,
            kws_config.detector.eager,
        ));
        let mut spotter = rustpotter::Rustpotter::new(&kws_config).map_err(|error| {
            wake_error(
                WAKE_ENGINE_INIT_FAILED,
                "로컬 Wake Word 엔진을 초기화하지 못했습니다.",
                error,
            )
        })?;
        wake_log("Wake Word Engine init: success (rustpotter)");
        use rustpotter::WakewordLoad;
        let reference_path = crate::wake_word_setup::active_reference_path(&app)?;
        let model = crate::wake_word_setup::reference_metadata(&reference_path)?;
        wake_log(format!(
            "reference diagnostics: instance_id={listener_id}, model_rms={:.6}, reference_mfcc_frames={:?}, average_mfcc_frames={:?}; subthreshold scores unavailable in Rustpotter public API",
            model.rms_level, model.reference_frames, model.average_frames,
        ));
        wake_log(format!(
            "active model selected: instance_id={listener_id}, model_id={}, references={}, model_threshold={:?}, model_avg_threshold={:?}",
            model.id,
            model.reference_count,
            model.threshold,
            model.avg_threshold,
        ));
        let reference_bytes =
            std::fs::read(&reference_path).map_err(|_| WAKE_ENGINE_MODEL_LOAD_FAILED.to_owned())?;
        let reference =
            rustpotter::WakewordRef::load_from_buffer(&reference_bytes).map_err(|error| {
                wake_error(
                    WAKE_ENGINE_MODEL_LOAD_FAILED,
                    "사용자 Wake Word reference를 불러오지 못했습니다.",
                    error,
                )
            })?;
        spotter
            .add_wakeword_ref("ACE", reference)
            .map_err(|error| {
                wake_error(
                    WAKE_ENGINE_MODEL_LOAD_FAILED,
                    "ACE 로컬 Wake Word reference를 등록하지 못했습니다.",
                    error,
                )
            })?;
        wake_log("Wake Word Model load: success (user reference)");

        let (audio_tx, audio_rx) = mpsc::sync_channel::<AudioChunk>(12);
        let (event_tx, event_rx) = mpsc::sync_channel::<ListenerEvent>(8);
        let metrics = Arc::new(AudioMetrics::new());
        let mut stream_config: cpal::StreamConfig = supported.into();
        stream_config.buffer_size = cpal::BufferSize::Default;
        wake_log(format!(
            "selected stream config: sample_rate={} Hz, channels={}, buffer_size={:?}",
            stream_config.sample_rate, stream_config.channels, stream_config.buffer_size
        ));
        let stream_started = Instant::now();
        let stream = match supported.sample_format() {
            cpal::SampleFormat::I16 => {
                let chunks = audio_tx.clone();
                let callback_metrics = Arc::clone(&metrics);
                let error_metrics = Arc::clone(&metrics);
                let errors = event_tx.clone();
                device.build_input_stream(
                    stream_config,
                    move |samples: &[i16], _| {
                        let started = Instant::now();
                        callback_metrics
                            .callback_count
                            .fetch_add(1, Ordering::Relaxed);
                        enqueue_audio_chunk(
                            &chunks,
                            &callback_metrics,
                            AudioChunk::I16(samples.to_vec()),
                        );
                        callback_metrics.callback_ns.fetch_add(
                            started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
                            Ordering::Relaxed,
                        );
                    },
                    move |error| {
                        handle_cpal_runtime_error(error, &error_metrics, &errors, stream_started);
                    },
                    None,
                )
            }
            cpal::SampleFormat::F32 => {
                let chunks = audio_tx.clone();
                let callback_metrics = Arc::clone(&metrics);
                let error_metrics = Arc::clone(&metrics);
                let errors = event_tx.clone();
                device.build_input_stream(
                    stream_config,
                    move |samples: &[f32], _| {
                        let started = Instant::now();
                        callback_metrics
                            .callback_count
                            .fetch_add(1, Ordering::Relaxed);
                        enqueue_audio_chunk(
                            &chunks,
                            &callback_metrics,
                            AudioChunk::F32(samples.to_vec()),
                        );
                        callback_metrics.callback_ns.fetch_add(
                            started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
                            Ordering::Relaxed,
                        );
                    },
                    move |error| {
                        handle_cpal_runtime_error(error, &error_metrics, &errors, stream_started);
                    },
                    None,
                )
            }
            _ => unreachable!(),
        }
        .map_err(|error| {
            let code = initialization_error_code(&error, WAKE_MIC_STREAM_BUILD_FAILED);
            wake_error(code, "Wake Word 마이크 스트림을 만들지 못했습니다.", error)
        })?;
        wake_log("build_input_stream: success");
        stream.play().map_err(|error| {
            let code = initialization_error_code(&error, WAKE_MIC_STREAM_START_FAILED);
            wake_error(
                code,
                "Wake Word 마이크 스트림을 시작하지 못했습니다.",
                error,
            )
        })?;
        wake_log("stream.play: success; local KWS listening");
        #[cfg(debug_assertions)]
        crate::wake_diagnostics::initialize(&kws_config, &reference_bytes);
        Ok((stream, spotter, audio_rx, event_rx, metrics, input_channels))
    })();

    let (stream, mut spotter, audio, events, metrics, input_channels) = match setup {
        Ok(value) => value,
        Err(error) => {
            wake_log(format!(
                "listener exited: instance_id={listener_id}, reason=initialization_failed, error_code={}",
                wake_error_code(&error)
            ));
            #[cfg(debug_assertions)]
            crate::wake_diagnostics::finish("initialization_failed");
            let _ = ready.send(Err(error));
            return;
        }
    };
    let stream = stream;
    let _ = ready.send(Ok(()));
    let mut pending_i16 = Vec::new();
    let mut pending_f32 = Vec::new();
    let mut score_diagnostics = KwsScoreDiagnostics::default();
    let diagnostics_started = Instant::now();
    let mut last_diagnostics = Instant::now();
    let mut last_xrun_count = 0;
    let mut last_dropped_chunks = 0;
    let mut paused_for_voice = false;
    let mut raw_levels = SignalLevels::default();
    let mut engine_levels = SignalLevels::default();
    let mut processed_frames = 0_u64;
    let mut final_detections = 0_u64;
    let mut exit_reason = "stop requested";
    loop {
        #[cfg(debug_assertions)]
        crate::wake_diagnostics::tick();
        if stop_rx.try_recv().is_ok() {
            break;
        }
        if paused_for_voice && !voice_overlay::is_active(&app) {
            match stream.play() {
                Ok(()) => {
                    paused_for_voice = false;
                    app.state::<WakeWordRuntime>().update(
                        &app,
                        WakeWordState::Listening,
                        true,
                        None,
                        None,
                    );
                    wake_log("Wake Word microphone resumed after voice recording");
                }
                Err(error) => {
                    exit_reason = "stream resume failed";
                    let code = initialization_error_code(&error, WAKE_MIC_STREAM_START_FAILED);
                    let _ = wake_error(
                        code,
                        "음성 녹음 후 Wake Word 마이크 스트림을 재시작하지 못했습니다.",
                        error,
                    );
                    app.state::<WakeWordRuntime>().update(
                        &app,
                        WakeWordState::Error,
                        true,
                        Some(wake_user_message(code)),
                        Some(code.to_owned()),
                    );
                    break;
                }
            }
        }
        if let Ok(ListenerEvent::FatalStreamError(kind, error)) = events.try_recv() {
            exit_reason = "fatal stream error";
            let code = match kind {
                cpal::ErrorKind::DeviceNotAvailable => WAKE_MIC_DEVICE_NOT_FOUND,
                cpal::ErrorKind::PermissionDenied => WAKE_MIC_PERMISSION_DENIED,
                cpal::ErrorKind::StreamInvalidated | cpal::ErrorKind::BackendError => {
                    WAKE_ENGINE_RUNTIME_FAILED
                }
                _ => WAKE_ENGINE_RUNTIME_FAILED,
            };
            let _ = wake_error(
                code,
                "Wake Word 마이크 스트림에 복구할 수 없는 오류가 발생했습니다.",
                format!("{kind:?}: {error}"),
            );
            app.state::<WakeWordRuntime>().update(
                &app,
                WakeWordState::Error,
                true,
                Some(wake_user_message(code)),
                Some(code.to_owned()),
            );
            break;
        }
        match audio.recv_timeout(Duration::from_millis(50)) {
            Ok(chunk) => {
                metrics.queue_depth.fetch_sub(1, Ordering::Relaxed);
                let started = Instant::now();
                #[cfg(debug_assertions)]
                let diagnostic_capture = crate::wake_diagnostics::active();
                #[cfg(not(debug_assertions))]
                let diagnostic_capture = false;
                let detected = match chunk {
                    AudioChunk::I16(samples) => {
                        raw_levels.observe_i16(&samples);
                        let mono = downmix_i16(&samples, input_channels);
                        engine_levels.observe_i16(&mono);
                        process_kws_samples(
                            &mut spotter,
                            &mut pending_i16,
                            &mono,
                            &mut score_diagnostics,
                            &mut processed_frames,
                        )
                    }
                    AudioChunk::F32(samples) => {
                        raw_levels.observe_f32(&samples);
                        let mono = downmix_f32(&samples, input_channels);
                        engine_levels.observe_f32(&mono);
                        process_kws_samples(
                            &mut spotter,
                            &mut pending_f32,
                            &mono,
                            &mut score_diagnostics,
                            &mut processed_frames,
                        )
                    }
                };
                metrics.worker_count.fetch_add(1, Ordering::Relaxed);
                metrics.worker_ns.fetch_add(
                    started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
                    Ordering::Relaxed,
                );
                if detected {
                    final_detections = final_detections.saturating_add(1);
                    if diagnostic_capture {
                        wake_log("diagnostic capture detection: Orb activation suppressed");
                    }
                }
                if detected && !diagnostic_capture {
                    match stream.pause() {
                        Ok(()) => {
                            if handle_detection(&app, "ACE", true) {
                                paused_for_voice = true;
                                app.state::<WakeWordRuntime>().update(
                                    &app,
                                    WakeWordState::Paused,
                                    true,
                                    None,
                                    None,
                                );
                                wake_log(
                                    "Wake Word microphone paused while voice overlay is active",
                                );
                            } else if let Err(error) = stream.play() {
                                wake_log(format!(
                                    "Wake Word stream resume after ignored detection failed: {error}"
                                ));
                            }
                        }
                        Err(error) => {
                            wake_log(format!(
                                "Wake Word stream pause failed; activation continues: {error}"
                            ));
                            let _ = handle_detection(&app, "ACE", true);
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                exit_reason = "audio queue disconnected";
                break;
            }
        }

        let xrun_count = metrics.xrun_count.load(Ordering::Relaxed);
        let dropped_chunks = metrics.dropped_chunks.load(Ordering::Relaxed);
        let first_xrun = xrun_count > 0 && last_xrun_count == 0;
        let diagnostics_due = last_diagnostics.elapsed() >= Duration::from_secs(10);
        let metrics_changed =
            xrun_count != last_xrun_count || dropped_chunks != last_dropped_chunks;
        if first_xrun || (diagnostics_due && (cfg!(debug_assertions) || metrics_changed)) {
            let callbacks = metrics.callback_count.load(Ordering::Relaxed);
            let workers = metrics.worker_count.load(Ordering::Relaxed);
            let callback_us =
                metrics.callback_ns.load(Ordering::Relaxed) / callbacks.max(1) / 1_000;
            let worker_us = metrics.worker_ns.load(Ordering::Relaxed) / workers.max(1) / 1_000;
            let elapsed_seconds = diagnostics_started.elapsed().as_secs_f64().max(0.001);
            let first_xrun_ms = metrics.first_xrun_ms.load(Ordering::Relaxed);
            wake_log(format!(
                "audio diagnostics: callbacks={callbacks}, avg_callback={callback_us}us, workers={workers}, avg_worker={worker_us}us, queue_depth={}, dropped={}, {WAKE_MIC_XRUN}={}, first_xrun_ms={}, xrun_frequency={:.3}/s",
                metrics.queue_depth.load(Ordering::Relaxed),
                dropped_chunks,
                xrun_count,
                if first_xrun_ms == u64::MAX { 0 } else { first_xrun_ms },
                xrun_count as f64 / elapsed_seconds,
            ));
            last_xrun_count = xrun_count;
            last_dropped_chunks = dropped_chunks;
            last_diagnostics = Instant::now();
        }
        if diagnostics_due && cfg!(debug_assertions) {
            let (raw_rms, raw_peak, raw_samples) = raw_levels.take();
            let (engine_rms, engine_peak, engine_samples) = engine_levels.take();
            wake_log(format!(
                "engine input diagnostics: instance_id={listener_id}, raw_rms={raw_rms:.6}, raw_peak={raw_peak:.6}, raw_samples={raw_samples}, engine_rms={engine_rms:.6}, engine_peak={engine_peak:.6}, engine_samples={engine_samples}, rustpotter_rms={:.6}, rustpotter_gain={:.4}, processed_frames={processed_frames}, partial_active={}, final_detections={final_detections}",
                spotter.get_rms_level(),
                spotter.get_gain(),
                spotter.get_partial_detection().is_some(),
            ));
        }
    }
    wake_log(format!(
        "listener exited: instance_id={listener_id}, reason={exit_reason}, processed_frames={processed_frames}, final_detections={final_detections}"
    ));
    #[cfg(debug_assertions)]
    crate::wake_diagnostics::finish(exit_reason);
}

#[cfg(all(windows, debug_assertions))]
pub(crate) fn diagnostic_listener_state(app: &AppHandle) -> WakeWordState {
    app.state::<WakeWordRuntime>().status().state
}

#[cfg(not(windows))]
fn run_listener(
    _app: AppHandle,
    _listener_id: u64,
    _stop_rx: mpsc::Receiver<()>,
    ready: mpsc::SyncSender<Result<(), String>>,
) {
    let _ = ready.send(Err("Wake Word는 현재 Windows에서 지원됩니다.".into()));
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInputDiagnostic {
    host: String,
    input_device_count: usize,
    default_input_device: Option<String>,
    selected_input_device: Option<String>,
    sample_rate: Option<u32>,
    channels: Option<u16>,
    sample_format: Option<String>,
    buffer_size: Option<String>,
}

#[cfg(windows)]
pub(crate) fn collect_audio_input_diagnostic() -> Result<AudioInputDiagnostic, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(|error| {
            wake_error(
                WAKE_MIC_ENUMERATION_FAILED,
                "마이크 장치 목록을 읽지 못했습니다.",
                error,
            )
        })?
        .collect::<Vec<_>>();
    let default = host.default_input_device();
    let default_name = default
        .as_ref()
        .and_then(|device| device.description().ok())
        .map(|description| description.name().to_owned());
    let selected = default.or_else(|| {
        let mut compatible = devices.iter().filter(|device| {
            device.default_input_config().is_ok_and(|config| {
                matches!(
                    config.sample_format(),
                    cpal::SampleFormat::I16 | cpal::SampleFormat::F32
                )
            })
        });
        let first = compatible.next()?.clone();
        compatible.next().is_none().then_some(first)
    });
    let selected_name = selected
        .as_ref()
        .and_then(|device| device.description().ok())
        .map(|description| description.name().to_owned());
    let config = selected
        .as_ref()
        .and_then(|device| device.default_input_config().ok());
    Ok(AudioInputDiagnostic {
        host: host.id().name().to_owned(),
        input_device_count: devices.len(),
        default_input_device: default_name,
        selected_input_device: selected_name,
        sample_rate: config.as_ref().map(|value| value.sample_rate()),
        channels: config.as_ref().map(|value| value.channels()),
        sample_format: config
            .as_ref()
            .map(|value| format!("{:?}", value.sample_format())),
        buffer_size: config.as_ref().map(|_| "Default".to_owned()),
    })
}

#[cfg(not(windows))]
pub(crate) fn collect_audio_input_diagnostic() -> Result<AudioInputDiagnostic, String> {
    Err("Wake Word audio diagnostics are available on Windows.".into())
}

#[tauri::command]
pub fn diagnose_audio_input(window: tauri::WebviewWindow) -> Result<AudioInputDiagnostic, String> {
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    collect_audio_input_diagnostic()
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
    use std::path::PathBuf;

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

    #[test]
    fn transient_microphone_initialization_errors_are_retryable() {
        assert!(retryable_error(WAKE_MIC_DEVICE_NOT_FOUND));
        assert!(retryable_error(WAKE_MIC_ENUMERATION_FAILED));
        assert!(retryable_error(WAKE_MIC_CONFIG_FAILED));
        assert!(retryable_error(WAKE_MIC_STREAM_BUILD_FAILED));
        assert!(retryable_error(WAKE_MIC_STREAM_START_FAILED));
        assert!(!retryable_error(WAKE_MIC_PERMISSION_DENIED));
        assert!(!retryable_error(WAKE_ENGINE_MODEL_LOAD_FAILED));
    }

    #[cfg(windows)]
    #[test]
    fn stereo_microphone_frames_are_downmixed_to_mono() {
        assert_eq!(downmix_i16(&[1000, 3000, -2000, 2000], 2), vec![2000, 0]);
        assert_eq!(downmix_f32(&[0.2, 0.6, -0.4, 0.2], 2), vec![0.4, -0.1]);
    }

    #[cfg(windows)]
    #[test]
    fn reference_score_format_is_stable_and_readable() {
        let scores = vec![
            ("sample_1".to_owned(), 0.45678),
            ("sample_2".to_owned(), 0.9),
        ];
        assert_eq!(
            format_reference_scores(&scores),
            "sample_1:0.4568,sample_2:0.9000"
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Requires private reference WAV fixtures; intentionally excluded from publication"]
    fn bundled_reference_wav_quality_report() {
        const SILENCE_AMPLITUDE_RATIO: f64 = 0.01;
        const EXCESSIVE_SILENCE_SECONDS: f64 = 0.5;
        const EXCESSIVE_RMS_RATIO: f64 = 3.0;

        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/wake-word/samples");
        let mut paths = std::fs::read_dir(directory)
            .expect("reference directory must open")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "wav"))
            .collect::<Vec<_>>();
        paths.sort();
        assert_eq!(
            paths.len(),
            6,
            "six bundled reference WAV files are expected"
        );

        let mut rms_values = Vec::new();
        for path in paths {
            let mut reader = hound::WavReader::open(&path).expect("reference WAV must open");
            let spec = reader.spec();
            assert_eq!(spec.sample_format, hound::SampleFormat::Int);
            assert_eq!(spec.bits_per_sample, 16);
            let samples = reader
                .samples::<i16>()
                .collect::<Result<Vec<_>, _>>()
                .expect("reference WAV samples must decode");
            assert!(!samples.is_empty(), "reference WAV must contain samples");

            let full_scale = i16::MAX as f64;
            let silence_limit = (full_scale * SILENCE_AMPLITUDE_RATIO).round() as i16;
            let leading_samples = samples
                .iter()
                .take_while(|sample| sample.unsigned_abs() <= silence_limit as u16)
                .count();
            let trailing_samples = samples
                .iter()
                .rev()
                .take_while(|sample| sample.unsigned_abs() <= silence_limit as u16)
                .count();
            let sample_denominator = spec.sample_rate as f64 * spec.channels as f64;
            let duration = samples.len() as f64 / sample_denominator;
            let leading_silence = leading_samples as f64 / sample_denominator;
            let trailing_silence = trailing_samples as f64 / sample_denominator;
            let rms = (samples
                .iter()
                .map(|sample| (f64::from(*sample) / full_scale).powi(2))
                .sum::<f64>()
                / samples.len() as f64)
                .sqrt();
            let peak = samples
                .iter()
                .map(|sample| sample.unsigned_abs() as f64 / full_scale)
                .fold(0.0_f64, f64::max);
            let clipped = samples
                .iter()
                .any(|sample| matches!(*sample, i16::MIN | i16::MAX));
            rms_values.push(rms);

            eprintln!(
                "REFERENCE_WAV_QUALITY file={} duration_s={duration:.3} rms={rms:.6} peak={peak:.6} leading_silence_s={leading_silence:.3} trailing_silence_s={trailing_silence:.3} clipping={clipped}",
                path.file_name().unwrap_or_default().to_string_lossy(),
            );
            if leading_silence > EXCESSIVE_SILENCE_SECONDS
                || trailing_silence > EXCESSIVE_SILENCE_SECONDS
            {
                eprintln!(
                    "REFERENCE_WAV_WARNING file={} excessive_silence=true",
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
            }
            if clipped {
                eprintln!(
                    "REFERENCE_WAV_WARNING file={} clipping=true",
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
            }
        }

        let minimum_rms = rms_values.iter().copied().fold(f64::INFINITY, f64::min);
        let maximum_rms = rms_values.iter().copied().fold(0.0_f64, f64::max);
        let rms_ratio = maximum_rms / minimum_rms.max(f64::EPSILON);
        eprintln!(
            "REFERENCE_WAV_QUALITY_SUMMARY min_rms={minimum_rms:.6} max_rms={maximum_rms:.6} rms_ratio={rms_ratio:.3} excessive_volume_difference={}",
            rms_ratio > EXCESSIVE_RMS_RATIO
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Requires private reference WAV fixtures; intentionally excluded from publication"]
    fn bundled_local_kws_model_detects_reference_audio() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let reference_directory = root.join("resources/wake-word/samples");
        let sample = reference_directory.join("ace_1.wav");
        let mut reader = hound::WavReader::open(sample).expect("reference WAV must open");
        let specification = reader.spec();
        let mut config = rustpotter::RustpotterConfig::default();
        config.fmt.sample_rate = specification.sample_rate as usize;
        config.fmt.channels = specification.channels;
        config.fmt.sample_format = rustpotter::SampleFormat::I16;
        config.detector.threshold = 0.52;
        config.detector.avg_threshold = 0.22;
        config.detector.min_scores = 2;
        config.detector.eager = false;
        config.filters.gain_normalizer.enabled = true;
        let mut spotter = rustpotter::Rustpotter::new(&config).expect("KWS engine must initialize");
        use rustpotter::WakewordRefBuildFromFiles;
        let mut references = std::fs::read_dir(reference_directory)
            .expect("reference directory must open")
            .filter_map(Result::ok)
            .map(|entry| entry.path().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        references.sort();
        let reference = rustpotter::WakewordRef::new_from_sample_files(
            "ACE".into(),
            Some(0.52),
            Some(0.22),
            references,
            13,
        )
        .expect("bundled ACE references must load");
        spotter
            .add_wakeword_ref("ACE", reference)
            .expect("bundled ACE model must register");
        let frame_size = spotter.get_samples_per_frame();
        let mut samples = reader
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .expect("reference samples must decode");
        samples.extend(std::iter::repeat_n(
            0,
            specification.sample_rate as usize * specification.channels as usize * 3,
        ));
        let detected = samples
            .chunks_exact(frame_size)
            .any(|frame| spotter.process_samples(frame.to_vec()).is_some());
        assert!(
            detected,
            "bundled ACE model must detect its reference audio"
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Requires private reference WAV fixtures; intentionally excluded from publication"]
    fn offline_i16_and_live_f32_paths_detect_the_same_reference_audio() {
        use rustpotter::WakewordRefBuildFromFiles;

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let reference_directory = root.join("resources/wake-word/samples");
        let sample_path = reference_directory.join("ace_1.wav");
        let mut reader = hound::WavReader::open(sample_path).expect("reference WAV must open");
        let specification = reader.spec();
        let source = reader
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .expect("reference samples must decode");
        let mut reference_paths = std::fs::read_dir(reference_directory)
            .expect("reference directory must open")
            .filter_map(Result::ok)
            .map(|entry| entry.path().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        reference_paths.sort();
        let build_reference = || {
            rustpotter::WakewordRef::new_from_sample_files(
                "ACE".into(),
                Some(0.52),
                Some(0.22),
                reference_paths.clone(),
                13,
            )
            .expect("bundled ACE references must load")
        };
        let configure = |sample_rate: usize, sample_format| {
            let mut config = rustpotter::RustpotterConfig::default();
            config.fmt.sample_rate = sample_rate;
            config.fmt.channels = 1;
            config.fmt.sample_format = sample_format;
            config.filters.gain_normalizer.enabled = true;
            config.detector.threshold = 0.52;
            config.detector.avg_threshold = 0.22;
            config.detector.min_scores = 2;
            config.detector.eager = false;
            config
        };

        let mut offline = rustpotter::Rustpotter::new(&configure(
            specification.sample_rate as usize,
            rustpotter::SampleFormat::I16,
        ))
        .expect("offline detector must initialize");
        offline
            .add_wakeword_ref("ACE", build_reference())
            .expect("offline model must register");
        let mut offline_samples = source.clone();
        offline_samples.extend(std::iter::repeat_n(
            0,
            specification.sample_rate as usize * 3,
        ));
        let offline_detected = offline_samples
            .chunks_exact(offline.get_samples_per_frame())
            .any(|frame| offline.process_samples(frame.to_vec()).is_some());

        const LIVE_RATE: usize = 48_000;
        let source_rate = specification.sample_rate as usize;
        let output_length = source.len() * LIVE_RATE / source_rate;
        let resampled = (0..output_length)
            .map(|index| {
                let position = index as f64 * source_rate as f64 / LIVE_RATE as f64;
                let left = position.floor() as usize;
                let right = (left + 1).min(source.len().saturating_sub(1));
                let fraction = (position - left as f64) as f32;
                let a = f32::from(source[left]) / i16::MAX as f32;
                let b = f32::from(source[right]) / i16::MAX as f32;
                a + (b - a) * fraction
            })
            .collect::<Vec<_>>();
        let stereo = resampled
            .iter()
            .flat_map(|sample| [*sample, *sample])
            .collect::<Vec<_>>();
        let mut live_samples = downmix_f32(&stereo, 2);
        live_samples.extend(std::iter::repeat_n(0.0, LIVE_RATE * 3));
        let mut live =
            rustpotter::Rustpotter::new(&configure(LIVE_RATE, rustpotter::SampleFormat::F32))
                .expect("live detector must initialize");
        live.add_wakeword_ref("ACE", build_reference())
            .expect("live model must register");
        let live_detected = live_samples
            .chunks_exact(live.get_samples_per_frame())
            .any(|frame| live.process_samples(frame.to_vec()).is_some());

        assert!(
            offline_detected,
            "offline setup path must detect the sample"
        );
        assert!(
            live_detected,
            "live F32/downmix path must detect the same sample"
        );
    }

    #[cfg(windows)]
    #[test]
    fn xrun_is_counted_without_becoming_fatal() {
        let metrics = AudioMetrics::new();
        let (events, receiver) = mpsc::sync_channel(1);
        handle_cpal_runtime_error(
            cpal::Error::new(cpal::ErrorKind::Xrun),
            &metrics,
            &events,
            Instant::now(),
        );
        assert_eq!(metrics.xrun_count.load(Ordering::Relaxed), 1);
        assert!(receiver.try_recv().is_err());
    }

    #[cfg(windows)]
    #[test]
    fn unavailable_device_is_forwarded_as_fatal() {
        let metrics = AudioMetrics::new();
        let (events, receiver) = mpsc::sync_channel(1);
        handle_cpal_runtime_error(
            cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable),
            &metrics,
            &events,
            Instant::now(),
        );
        assert!(matches!(
            receiver.try_recv(),
            Ok(ListenerEvent::FatalStreamError(
                cpal::ErrorKind::DeviceNotAvailable,
                _
            ))
        ));
    }

    #[cfg(windows)]
    #[test]
    fn full_audio_queue_drops_chunk_without_blocking() {
        let metrics = AudioMetrics::new();
        let (chunks, _receiver) = mpsc::sync_channel(1);
        enqueue_audio_chunk(&chunks, &metrics, AudioChunk::I16(vec![1, 2]));
        enqueue_audio_chunk(&chunks, &metrics, AudioChunk::I16(vec![3, 4]));
        assert_eq!(metrics.queue_depth.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.dropped_chunks.load(Ordering::Relaxed), 1);
    }
}
