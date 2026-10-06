use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::BufWriter,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Manager};

#[cfg(windows)]
use sha2::{Digest, Sha256};

const SAMPLE_TOTAL: usize = 5;
const MIN_DURATION_MS: u64 = 350;
const MAX_DURATION_MS: u64 = 3_500;
const MIN_RMS: f64 = 0.008;
const MAX_CLIPPING_RATIO: f64 = 0.02;
const SILENCE_AMPLITUDE: f32 = 0.01;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum WakeSensitivity {
    #[default]
    Standard,
    Sensitive,
}

impl WakeSensitivity {
    #[cfg(windows)]
    pub(crate) fn apply(self, reference: &mut rustpotter::WakewordRef) {
        if self == Self::Sensitive {
            // Preserve an already more sensitive personal model.
            reference.threshold = Some(reference.threshold.unwrap_or(0.52).min(0.48));
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceActivationSettings {
    pub enabled: bool,
    pub setup_completed: bool,
    pub reference_exists: bool,
    #[serde(default)]
    pub sensitivity: WakeSensitivity,
    #[serde(default = "legacy_personal_preference")]
    pub prefer_personal: bool,
    #[serde(default, skip_deserializing)]
    pub default_available: bool,
    #[serde(default, skip_deserializing)]
    pub model_source: Option<WakeModelSource>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum WakeModelSource {
    Default,
    Personal,
}

fn legacy_personal_preference() -> bool {
    true
}

impl Default for VoiceActivationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            setup_completed: false,
            reference_exists: false,
            sensitivity: WakeSensitivity::Standard,
            prefer_personal: false,
            default_available: false,
            model_source: None,
        }
    }
}

const DEFAULT_REFERENCE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ace-default-wake.rpw"));

fn default_reference_valid() -> bool {
    #[cfg(windows)]
    {
        use rustpotter::WakewordLoad;
        rustpotter::WakewordRef::load_from_buffer(DEFAULT_REFERENCE).is_ok()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn reconcile_models(
    mut settings: VoiceActivationSettings,
    personal_valid: bool,
    default_valid: bool,
) -> VoiceActivationSettings {
    let personal_ready = personal_valid && settings.setup_completed;
    settings.default_available = default_valid;
    settings.model_source = if settings.prefer_personal && personal_ready {
        Some(WakeModelSource::Personal)
    } else if default_valid {
        Some(WakeModelSource::Default)
    } else if personal_ready {
        Some(WakeModelSource::Personal)
    } else {
        None
    };
    settings.reference_exists = settings.model_source.is_some();
    if !personal_valid {
        settings.setup_completed = false;
    }
    if !settings.reference_exists {
        settings.enabled = false;
    }
    settings
}

pub fn selected_reference_path(app: &AppHandle) -> Result<PathBuf, String> {
    match load_settings(app).model_source {
        Some(WakeModelSource::Personal) => active_reference_path(app),
        Some(WakeModelSource::Default) => {
            let path = wake_dir(app)?.join("default-reference.rpw");
            fs::create_dir_all(wake_dir(app)?).map_err(|_| "WAKE_SETUP_STORAGE_FAILED")?;
            if fs::read(&path).ok().as_deref() != Some(DEFAULT_REFERENCE) {
                let temporary = path.with_extension("rpw.tmp");
                fs::write(&temporary, DEFAULT_REFERENCE).map_err(|_| "WAKE_SETUP_STORAGE_FAILED")?;
                fs::rename(temporary, &path).map_err(|_| "WAKE_SETUP_STORAGE_FAILED")?;
            }
            Ok(path)
        }
        None => Err("사용 가능한 호출 모델이 없습니다. 기본 모델이 포함된 앱을 설치하거나 개인 보정을 진행해 주세요.".into()),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupProgress {
    pub state: &'static str,
    pub completed_samples: usize,
    pub total_samples: usize,
    pub settings: VoiceActivationSettings,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleQuality {
    pub accepted: bool,
    pub code: Option<&'static str>,
    pub message: Option<&'static str>,
    pub duration_ms: u64,
    pub rms: f64,
    pub peak: f64,
    pub clipping_ratio: f64,
}

struct SetupRecording {
    writer: hound::WavWriter<BufWriter<File>>,
    path: PathBuf,
    sample_rate: u32,
    samples: u64,
    sum_squares: f64,
    peak: f64,
    clipped: u64,
    voiced: u64,
}

#[derive(Default)]
pub struct WakeWordSetupRuntime {
    recording: Mutex<Option<SetupRecording>>,
    #[cfg(windows)]
    capture: Mutex<Option<crate::wake_setup_capture::Capture>>,
    device_identity: Mutex<Option<String>>,
    test_result: Mutex<Option<bool>>,
    successful_tests: Mutex<usize>,
}

fn require_main(window: &tauri::WebviewWindow) -> Result<(), String> {
    (window.label() == "main")
        .then_some(())
        .ok_or_else(|| "BLOCKED".to_owned())
}

fn wake_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|path| path.join("wake-word"))
        .map_err(|_| "WAKE_SETUP_STORAGE_FAILED".to_owned())
}

fn staging_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(wake_dir(app)?.join("setup-staging"))
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(wake_dir(app)?.join("settings.json"))
}

pub fn active_reference_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(wake_dir(app)?.join("user-reference.rpw"))
}

#[cfg(windows)]
pub(crate) struct ReferenceMetadata {
    pub id: String,
    pub reference_count: usize,
    pub threshold: Option<f32>,
    pub avg_threshold: Option<f32>,
    pub rms_level: f32,
    pub reference_frames: Vec<usize>,
    pub average_frames: Option<usize>,
}

#[cfg(windows)]
pub(crate) fn reference_metadata(path: &Path) -> Result<ReferenceMetadata, String> {
    use rustpotter::WakewordLoad;

    let bytes = fs::read(path).map_err(|_| "WAKE_ENGINE_MODEL_LOAD_FAILED".to_owned())?;
    let id = format!("{:x}", Sha256::digest(&bytes));
    let reference = rustpotter::WakewordRef::load_from_buffer(&bytes)
        .map_err(|_| "WAKE_ENGINE_MODEL_LOAD_FAILED".to_owned())?;
    let mut reference_frames: Vec<_> = reference.samples_features.values().map(Vec::len).collect();
    reference_frames.sort_unstable();
    Ok(ReferenceMetadata {
        id,
        reference_count: reference.samples_features.len(),
        threshold: reference.threshold,
        avg_threshold: reference.avg_threshold,
        rms_level: reference.rms_level,
        reference_frames,
        average_frames: reference.avg_features.as_ref().map(Vec::len),
    })
}

fn candidate_reference_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(staging_dir(app)?.join("candidate-reference.rpw"))
}

pub fn load_settings(app: &AppHandle) -> VoiceActivationSettings {
    let settings = settings_path(app)
        .ok()
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<VoiceActivationSettings>(&bytes).ok())
        .unwrap_or_default();
    let reference_valid = active_reference_path(app)
        .ok()
        .is_some_and(|path| validate_reference_file(&path));
    reconcile_models(settings, reference_valid, default_reference_valid())
}

pub fn save_settings(app: &AppHandle, settings: &VoiceActivationSettings) -> Result<(), String> {
    let directory = wake_dir(app)?;
    fs::create_dir_all(&directory).map_err(|_| "WAKE_SETUP_STORAGE_FAILED")?;
    let path = settings_path(app)?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(settings).map_err(|_| "WAKE_SETUP_STORAGE_FAILED")?;
    fs::write(&temporary, bytes).map_err(|_| "WAKE_SETUP_STORAGE_FAILED")?;
    fs::rename(&temporary, &path).map_err(|_| "WAKE_SETUP_STORAGE_FAILED".to_owned())
}

#[cfg(windows)]
fn validate_reference_file(path: &Path) -> bool {
    use rustpotter::WakewordLoad;
    path.is_file() && rustpotter::WakewordRef::load_from_file(&path.to_string_lossy()).is_ok()
}

#[cfg(not(windows))]
fn validate_reference_file(path: &Path) -> bool {
    path.is_file()
}

fn sample_paths(app: &AppHandle) -> Result<Vec<PathBuf>, String> {
    let directory = staging_dir(app)?;
    let mut paths = (1..=SAMPLE_TOTAL)
        .map(|index| directory.join(format!("sample_{index:02}.wav")))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn progress(app: &AppHandle, state: &'static str) -> SetupProgress {
    SetupProgress {
        state,
        completed_samples: sample_paths(app).map_or(0, |paths| paths.len()),
        total_samples: SAMPLE_TOTAL,
        settings: load_settings(app),
    }
}

#[tauri::command]
pub fn get_wake_word_setup_status(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<SetupProgress, String> {
    require_main(&window)?;
    Ok(progress(&app, "idle"))
}

#[tauri::command]
pub fn prepare_wake_word_setup(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<SetupProgress, String> {
    require_main(&window)?;
    crate::wake_word::stop_for_setup(&app);
    cancel_setup_files(&app)?;
    fs::create_dir_all(staging_dir(&app)?).map_err(|_| "WAKE_SETUP_STORAGE_FAILED")?;
    crate::wake_word::collect_audio_input_diagnostic()?;
    Ok(progress(&app, "ready"))
}

#[tauri::command]
pub fn start_wake_word_sample(app: AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    require_main(&window)?;
    let paths = sample_paths(&app)?;
    if paths.len() >= SAMPLE_TOTAL {
        return Err("WAKE_SAMPLE_LIMIT_REACHED".into());
    }
    let path = staging_dir(&app)?.join(format!("sample_{:02}.wav", paths.len() + 1));
    begin_native_recording(&app, path, false)
}

fn begin_recording(app: &AppHandle, path: PathBuf, sample_rate: u32) -> Result<(), String> {
    let runtime = app.state::<WakeWordSetupRuntime>();
    let mut recording = runtime
        .recording
        .lock()
        .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?;
    if recording.is_some() {
        return Err("WAKE_SAMPLE_ALREADY_RECORDING".into());
    }
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let writer = hound::WavWriter::create(&path, spec)
        .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED".to_owned())?;
    *recording = Some(SetupRecording {
        writer,
        path,
        sample_rate,
        samples: 0,
        sum_squares: 0.0,
        peak: 0.0,
        clipped: 0,
        voiced: 0,
    });
    Ok(())
}

#[tauri::command]
pub fn start_wake_word_test_sample(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    require_main(&window)?;
    if !candidate_reference_path(&app)?.is_file() {
        return Err("WAKE_TEST_NOT_READY".into());
    }
    begin_native_recording(&app, staging_dir(&app)?.join("test.wav"), true)
}

fn begin_native_recording(app: &AppHandle, path: PathBuf, test: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let runtime = app.state::<WakeWordSetupRuntime>();
        let mut capture = runtime
            .capture
            .lock()
            .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?;
        if capture.is_some() {
            return Err("WAKE_SAMPLE_ALREADY_RECORDING".into());
        }
        *runtime.test_result.lock().map_err(|_| "WAKE_TEST_FAILED")? = None;
        let reference = if test {
            Some(candidate_reference_path(app)?)
        } else {
            None
        };
        let (next, rate, identity) = crate::wake_setup_capture::Capture::start(reference)?;
        let mut expected = runtime
            .device_identity
            .lock()
            .map_err(|_| "WAKE_MIC_CONFIG_FAILED")?;
        if expected.as_ref().is_some_and(|old| old != &identity) {
            return Err("등록 중 마이크가 변경됐습니다. 목소리 등록을 다시 시작해 주세요.".into());
        }
        *expected = Some(identity);
        begin_recording(app, path, rate)?;
        *capture = Some(next);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, path, test);
        Err("WAKE_SETUP_UNSUPPORTED".into())
    }
}

fn append_samples(app: &AppHandle, samples: &[f32]) -> Result<(), String> {
    let runtime = app.state::<WakeWordSetupRuntime>();
    let mut guard = runtime
        .recording
        .lock()
        .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?;
    let recording = guard.as_mut().ok_or("WAKE_SAMPLE_NOT_RECORDING")?;
    for &sample in samples {
        if !sample.is_finite() {
            return Err("WAKE_SAMPLE_FORMAT_INVALID".into());
        }
        let normalized = sample.clamp(-1.0, 1.0);
        recording.sum_squares += f64::from(normalized).powi(2);
        recording.peak = recording.peak.max(f64::from(normalized.abs()));
        recording.clipped += u64::from(normalized.abs() >= 0.99);
        recording.voiced += u64::from(normalized.abs() >= SILENCE_AMPLITUDE);
        recording
            .writer
            .write_sample((normalized * i16::MAX as f32).round() as i16)
            .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?;
        recording.samples += 1;
    }
    Ok(())
}

// 10ms energy windows with 120ms padding preserve quiet consonants. This is
// endpoint trimming, not a claim that energy alone recognizes speech.
fn speech_bounds(samples: &[f32], rate: u32) -> std::ops::Range<usize> {
    let frame = (rate as usize / 100).max(1);
    let energies: Vec<f32> = samples
        .chunks(frame)
        .map(|chunk| (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt())
        .collect();
    let peak = energies.iter().copied().fold(0.0_f32, f32::max);
    let threshold = (peak * 0.08).max(0.003);
    let Some(first) = energies.iter().position(|&rms| rms >= threshold) else {
        return 0..samples.len();
    };
    let last = energies.iter().rposition(|&rms| rms >= threshold).unwrap();
    let padding = rate as usize * 120 / 1000;
    (first * frame).saturating_sub(padding)..((last + 1) * frame + padding).min(samples.len())
}

fn quality(recording: &SetupRecording) -> SampleQuality {
    let duration_ms = recording.samples.saturating_mul(1000) / u64::from(recording.sample_rate);
    let rms = (recording.sum_squares / recording.samples.max(1) as f64).sqrt();
    let clipping_ratio = recording.clipped as f64 / recording.samples.max(1) as f64;
    let voiced_ms = recording.voiced.saturating_mul(1000) / u64::from(recording.sample_rate);
    let failure = if duration_ms < MIN_DURATION_MS {
        Some((
            "WAKE_SAMPLE_TOO_SHORT",
            "‘ACE’를 끝까지 자연스럽게 말해주세요.",
        ))
    } else if duration_ms > MAX_DURATION_MS {
        Some((
            "WAKE_SAMPLE_TOO_LONG",
            "한 번만 ‘ACE’라고 말한 뒤 녹음을 끝내주세요.",
        ))
    } else if voiced_ms < 180 || rms < 0.002 {
        Some((
            "WAKE_SAMPLE_SILENT",
            "목소리가 감지되지 않았습니다. ‘ACE’라고 말해주세요.",
        ))
    } else if rms < MIN_RMS {
        Some((
            "WAKE_SAMPLE_TOO_QUIET",
            "목소리가 너무 작습니다. 마이크에 조금 더 가까이 말해주세요.",
        ))
    } else if clipping_ratio > MAX_CLIPPING_RATIO {
        Some((
            "WAKE_SAMPLE_CLIPPED",
            "소리가 너무 크게 녹음되었습니다. 조금 떨어져 다시 말해주세요.",
        ))
    } else {
        None
    };
    SampleQuality {
        accepted: failure.is_none(),
        code: failure.map(|value| value.0),
        message: failure.map(|value| value.1),
        duration_ms,
        rms,
        peak: recording.peak,
        clipping_ratio,
    }
}

#[tauri::command]
pub fn finish_wake_word_sample(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<SampleQuality, String> {
    require_main(&window)?;
    #[cfg(windows)]
    {
        let runtime = app.state::<WakeWordSetupRuntime>();
        let capture = runtime
            .capture
            .lock()
            .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?
            .take()
            .ok_or("WAKE_SAMPLE_NOT_RECORDING")?;
        let captured = match capture.finish() {
            Ok(captured) => captured,
            Err(error) => {
                *runtime
                    .successful_tests
                    .lock()
                    .map_err(|_| "WAKE_TEST_FAILED")? = 0;
                if let Some(recording) = runtime
                    .recording
                    .lock()
                    .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?
                    .take()
                {
                    let path = recording.path.clone();
                    drop(recording);
                    let _ = fs::remove_file(path);
                }
                return Err(error);
            }
        };
        let (rate, test) = {
            let guard = runtime
                .recording
                .lock()
                .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?;
            let recording = guard.as_ref().ok_or("WAKE_SAMPLE_NOT_RECORDING")?;
            (
                recording.sample_rate,
                recording
                    .path
                    .file_name()
                    .is_some_and(|name| name == "test.wav"),
            )
        };
        let bounds = speech_bounds(&captured.samples, rate);
        crate::wake_word::wake_log(format!(
            "setup endpoints: raw_samples={}, retained_samples={}, live_test={test}",
            captured.samples.len(),
            bounds.len()
        ));
        append_samples(&app, &captured.samples[bounds])?;
        if test {
            *runtime.test_result.lock().map_err(|_| "WAKE_TEST_FAILED")? = Some(captured.detected);
        }
    }
    let mut recording = app
        .state::<WakeWordSetupRuntime>()
        .recording
        .lock()
        .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?
        .take()
        .ok_or("WAKE_SAMPLE_NOT_RECORDING")?;
    let result = quality(&recording);
    recording
        .writer
        .flush()
        .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?;
    recording
        .writer
        .finalize()
        .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?;
    #[cfg(all(windows, debug_assertions))]
    if let Err(error) = crate::wake_diagnostics::retain_sample(&app, &recording.path) {
        crate::wake_word::wake_log(format!("diagnostic sample retention failed: {error}"));
    }
    if !result.accepted {
        let _ = fs::remove_file(recording.path);
        let runtime = app.state::<WakeWordSetupRuntime>();
        *runtime.test_result.lock().map_err(|_| "WAKE_TEST_FAILED")? = None;
        *runtime
            .successful_tests
            .lock()
            .map_err(|_| "WAKE_TEST_FAILED")? = 0;
    }
    #[cfg(windows)]
    crate::wake_word::wake_log(format!(
        "setup sample finalized: accepted={}, sample_rate={}, channels=1, sample_format=I16, duration_ms={}, rms={:.6}, peak={:.6}, clipping_ratio={:.6}",
        result.accepted,
        recording.sample_rate,
        result.duration_ms,
        result.rms,
        result.peak,
        result.clipping_ratio,
    ));
    Ok(result)
}

#[tauri::command]
pub fn generate_wake_word_reference(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<SetupProgress, String> {
    require_main(&window)?;
    *app.state::<WakeWordSetupRuntime>()
        .successful_tests
        .lock()
        .map_err(|_| "WAKE_TEST_FAILED")? = 0;
    *app.state::<WakeWordSetupRuntime>()
        .test_result
        .lock()
        .map_err(|_| "WAKE_TEST_FAILED")? = None;
    let samples = sample_paths(&app)?;
    if samples.len() != SAMPLE_TOTAL {
        return Err("WAKE_SETUP_SAMPLES_INCOMPLETE".into());
    }
    #[cfg(windows)]
    {
        use rustpotter::{WakewordRefBuildFromFiles, WakewordSave};
        let reference = rustpotter::WakewordRef::new_from_sample_files(
            "ACE".into(),
            Some(0.52),
            Some(0.22),
            samples
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
            13,
        )
        .map_err(|_| "WAKE_REFERENCE_GENERATION_FAILED")?;
        reference
            .save_to_file(&candidate_reference_path(&app)?.to_string_lossy())
            .map_err(|_| "WAKE_REFERENCE_GENERATION_FAILED")?;
        let model = reference_metadata(&candidate_reference_path(&app)?)?;
        crate::wake_word::wake_log(format!(
            "candidate model generated: model_id={}, references={}, model_threshold={:?}, model_avg_threshold={:?}, source_samples={}",
            model.id,
            model.reference_count,
            model.threshold,
            model.avg_threshold,
            samples.len(),
        ));
    }
    #[cfg(not(windows))]
    return Err("WAKE_SETUP_UNSUPPORTED".into());
    Ok(progress(&app, "testing"))
}

#[tauri::command]
pub fn test_wake_word_reference(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<bool, String> {
    require_main(&window)?;
    let runtime = app.state::<WakeWordSetupRuntime>();
    let detected = runtime
        .test_result
        .lock()
        .map_err(|_| "WAKE_TEST_FAILED")?
        .take()
        .ok_or("WAKE_TEST_NOT_READY")?;
    let mut passes = runtime
        .successful_tests
        .lock()
        .map_err(|_| "WAKE_TEST_FAILED")?;
    if detected {
        *passes += 1;
    } else {
        *passes = 0;
    }
    #[cfg(windows)]
    crate::wake_word::wake_log(format!("native live onboarding test: detected={detected}, consecutive_passes={passes}, required=2; no synthetic silence"));
    Ok(detected)
}

#[tauri::command]
pub fn complete_wake_word_setup(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<VoiceActivationSettings, String> {
    require_main(&window)?;
    if *app
        .state::<WakeWordSetupRuntime>()
        .successful_tests
        .lock()
        .map_err(|_| "WAKE_TEST_FAILED")?
        < 2
    {
        return Err("WAKE_LIVE_TEST_REQUIRED".into());
    }
    let candidate = candidate_reference_path(&app)?;
    if !validate_reference_file(&candidate) {
        return Err("WAKE_REFERENCE_INVALID".into());
    }
    let active = active_reference_path(&app)?;
    #[cfg(windows)]
    let candidate_model = reference_metadata(&candidate)?;
    let backup = active.with_extension("rpw.backup");
    if active.exists() {
        let _ = fs::remove_file(&backup);
        fs::rename(&active, &backup).map_err(|_| "WAKE_SETUP_STORAGE_FAILED")?;
    }
    if let Err(error) = fs::rename(&candidate, &active) {
        if backup.exists() {
            let _ = fs::rename(&backup, &active);
        }
        return Err(format!("WAKE_SETUP_STORAGE_FAILED: {error}"));
    }
    #[cfg(windows)]
    {
        let active_model = match reference_metadata(&active) {
            Ok(model) => model,
            Err(error) => {
                let _ = fs::remove_file(&active);
                if backup.exists() {
                    let _ = fs::rename(&backup, &active);
                }
                return Err(error);
            }
        };
        if active_model.id != candidate_model.id {
            let _ = fs::remove_file(&active);
            if backup.exists() {
                let _ = fs::rename(&backup, &active);
            }
            return Err("WAKE_REFERENCE_ACTIVATION_MISMATCH".into());
        }
        crate::wake_word::wake_log(format!(
            "active model committed: model_id={}, references={}",
            active_model.id, active_model.reference_count
        ));
    }
    let _ = fs::remove_file(backup);
    let settings = VoiceActivationSettings {
        enabled: true,
        setup_completed: true,
        reference_exists: true,
        sensitivity: WakeSensitivity::Standard,
        prefer_personal: true,
        ..VoiceActivationSettings::default()
    };
    save_settings(&app, &settings)?;
    let _ = fs::remove_dir_all(staging_dir(&app)?);
    crate::wake_word::set_enabled(&app, true)?;
    Ok(settings)
}

#[tauri::command]
pub fn postpone_wake_word_setup(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<VoiceActivationSettings, String> {
    require_main(&window)?;
    #[cfg(windows)]
    {
        let capture = app
            .state::<WakeWordSetupRuntime>()
            .capture
            .lock()
            .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?
            .take();
        drop(capture);
    }
    if let Some(recording) = app
        .state::<WakeWordSetupRuntime>()
        .recording
        .lock()
        .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?
        .take()
    {
        drop(recording);
    }
    let active = active_reference_path(&app)?;
    let candidate = candidate_reference_path(&app)?;
    let had_active_reference = validate_reference_file(&active);
    if !had_active_reference && validate_reference_file(&candidate) {
        fs::rename(&candidate, &active).map_err(|_| "WAKE_SETUP_STORAGE_FAILED")?;
    }
    let settings = VoiceActivationSettings {
        enabled: false,
        setup_completed: had_active_reference,
        reference_exists: validate_reference_file(&active),
        sensitivity: load_settings(&app).sensitivity,
        prefer_personal: load_settings(&app).prefer_personal,
        ..VoiceActivationSettings::default()
    };
    save_settings(&app, &settings)?;
    let _ = fs::remove_dir_all(staging_dir(&app)?);
    Ok(settings)
}

fn cancel_setup_files(app: &AppHandle) -> Result<(), String> {
    let runtime = app.state::<WakeWordSetupRuntime>();
    #[cfg(windows)]
    {
        let capture = runtime
            .capture
            .lock()
            .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?
            .take();
        drop(capture);
    }
    *runtime
        .device_identity
        .lock()
        .map_err(|_| "WAKE_MIC_CONFIG_FAILED")? = None;
    *runtime.test_result.lock().map_err(|_| "WAKE_TEST_FAILED")? = None;
    *runtime
        .successful_tests
        .lock()
        .map_err(|_| "WAKE_TEST_FAILED")? = 0;
    if let Ok(mut guard) = app.state::<WakeWordSetupRuntime>().recording.lock() {
        if let Some(recording) = guard.take() {
            drop(recording.writer);
            let _ = fs::remove_file(recording.path);
        }
    }
    let staging = staging_dir(app)?;
    if staging.exists() {
        fs::remove_dir_all(staging).map_err(|_| "WAKE_SETUP_STORAGE_FAILED")?;
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_wake_word_setup(app: AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    require_main(&window)?;
    cancel_setup_files(&app)?;
    crate::wake_word::restore_persisted(&app)
}

pub fn set_persisted_enabled(
    app: &AppHandle,
    enabled: bool,
) -> Result<VoiceActivationSettings, String> {
    let mut settings = load_settings(app);
    if enabled && !settings.reference_exists {
        return Err("사용 가능한 호출 모델이 없습니다. 기본 모델이 포함된 앱을 설치하거나 개인 보정을 진행해 주세요.".into());
    }
    settings.enabled = enabled;
    save_settings(app, &settings)?;
    Ok(settings)
}

#[tauri::command]
pub async fn set_wake_word_model(
    app: AppHandle,
    window: tauri::WebviewWindow,
    source: WakeModelSource,
) -> Result<VoiceActivationSettings, String> {
    require_main(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        if crate::voice_overlay::is_active(&app) || staging_dir(&app)?.exists() {
            return Err("음성 입력이나 개인 보정을 마친 뒤 변경해 주세요.".into());
        }
        let previous = load_settings(&app);
        match source {
            WakeModelSource::Default if !previous.default_available => {
                return Err("이 앱에는 기본 호출 모델이 포함되어 있지 않습니다.".into())
            }
            WakeModelSource::Personal if !previous.setup_completed => {
                return Err("저장된 개인 보정이 없습니다.".into())
            }
            _ => {}
        }
        let mut next = previous.clone();
        next.prefer_personal = source == WakeModelSource::Personal;
        crate::wake_word::stop(&app);
        let outcome =
            save_settings(&app, &next).and_then(|()| crate::wake_word::restore_persisted(&app));
        if let Err(error) = outcome {
            let _ = save_settings(&app, &previous);
            let _ = crate::wake_word::restore_persisted(&app);
            return Err(error);
        }
        Ok(load_settings(&app))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn set_wake_word_sensitivity(
    app: AppHandle,
    window: tauri::WebviewWindow,
    sensitivity: WakeSensitivity,
) -> Result<VoiceActivationSettings, String> {
    require_main(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        static UPDATE: Mutex<()> = Mutex::new(());
        let _guard = UPDATE.lock().map_err(|_| "WAKE_SETUP_STORAGE_FAILED")?;
        if crate::voice_overlay::is_active(&app) || staging_dir(&app)?.exists() {
            return Err("녹음 또는 목소리 등록을 마친 뒤 감도를 변경해 주세요.".into());
        }
        #[cfg(all(windows, debug_assertions))]
        if crate::wake_diagnostics::busy() {
            return Err("진단 녹음을 마친 뒤 감도를 변경해 주세요.".into());
        }
        let previous = load_settings(&app);
        if !previous.reference_exists {
            return Err("사용 가능한 호출 모델이 없습니다.".into());
        }
        let mut next = previous.clone();
        next.sensitivity = sensitivity;
        save_settings(&app, &next)?;
        crate::wake_word::stop(&app);
        if let Err(error) = crate::wake_word::restore_persisted(&app) {
            save_settings(&app, &previous)?;
            let _ = crate::wake_word::restore_persisted(&app);
            return Err(error);
        }
        Ok(next)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_trimming_removes_button_delay_and_keeps_padding() {
        let rate = 48_000;
        let mut input = vec![0.0001; rate * 3];
        input[rate..rate + rate / 2].fill(0.1);
        let bounds = speech_bounds(&input, rate as u32);
        assert_eq!(bounds, 42_240..77_760);
        assert!(input[bounds].contains(&0.1));
    }

    #[test]
    fn endpoint_trimming_preserves_quiet_edges_and_silence_is_still_rejected() {
        let mut input = vec![0.0; 16_000];
        input[3_200..11_200].fill(0.05);
        input[2_560..3_200].fill(0.002);
        let bounds = speech_bounds(&input, 16_000);
        assert!(bounds.start <= 2_560 && bounds.end >= 11_200);
        let silence = vec![0.0; 16_000];
        assert_eq!(speech_bounds(&silence, 16_000), 0..16_000);
        assert!(!quality_for(&silence, 16_000).accepted);
        assert_eq!(speech_bounds(&[], 48_000), 0..0);
    }

    #[test]
    fn old_settings_keep_the_existing_model_policy() {
        let settings: VoiceActivationSettings = serde_json::from_str(
            r#"{"enabled":true,"setupCompleted":true,"referenceExists":true}"#,
        )
        .unwrap();
        assert!(settings.enabled && settings.setup_completed && settings.reference_exists);
        assert_eq!(settings.sensitivity, WakeSensitivity::Standard);
        assert!(settings.prefer_personal);
        assert_eq!(
            reconcile_models(settings, true, true).model_source,
            Some(WakeModelSource::Personal)
        );
        assert!(serde_json::from_str::<WakeSensitivity>(r#""unrestricted""#).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn personal_sensitivity_changes_only_the_effective_reference_gate() {
        use rustpotter::{WakewordLoad, WakewordSave};
        let original = rustpotter::WakewordRef {
            name: "ACE".into(),
            threshold: Some(0.52),
            avg_threshold: Some(0.22),
            avg_features: Some(vec![vec![1.0; 13]; 4]),
            samples_features: [("sample".into(), vec![vec![1.0; 13]; 4])].into(),
            rms_level: 0.02,
            mfcc_size: 13,
        };
        let bytes = original.save_to_buffer().unwrap();
        let mut effective = rustpotter::WakewordRef::load_from_buffer(&bytes).unwrap();
        WakeSensitivity::Sensitive.apply(&mut effective);
        // The serialized effective reference is also what diagnostic replay loads.
        let replay =
            rustpotter::WakewordRef::load_from_buffer(&effective.save_to_buffer().unwrap())
                .unwrap();
        assert_eq!(replay.threshold, Some(0.48));
        assert_eq!(replay.avg_threshold, original.avg_threshold);
        assert_eq!(replay.samples_features, original.samples_features);
        assert_eq!(replay.avg_features, original.avg_features);
        let mut restored = rustpotter::WakewordRef::load_from_buffer(&bytes).unwrap();
        WakeSensitivity::Standard.apply(&mut restored);
        assert_eq!(restored.threshold, Some(0.52));
        restored.threshold = Some(0.44);
        WakeSensitivity::Sensitive.apply(&mut restored);
        assert_eq!(restored.threshold, Some(0.44));
    }

    fn quality_for(samples: &[f32], rate: u32) -> SampleQuality {
        let path =
            std::env::temp_dir().join(format!("ace-wake-quality-{}.wav", uuid::Uuid::new_v4()));
        let mut recording = SetupRecording {
            writer: hound::WavWriter::create(
                &path,
                hound::WavSpec {
                    channels: 1,
                    sample_rate: rate,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap(),
            path: PathBuf::new(),
            sample_rate: rate,
            samples: samples.len() as u64,
            sum_squares: 0.0,
            peak: 0.0,
            clipped: 0,
            voiced: 0,
        };
        for sample in samples {
            recording.sum_squares += f64::from(*sample).powi(2);
            recording.peak = recording.peak.max(f64::from(sample.abs()));
            recording.clipped += u64::from(sample.abs() >= 0.99);
            recording.voiced += u64::from(sample.abs() >= SILENCE_AMPLITUDE);
        }
        let result = quality(&recording);
        drop(recording);
        let _ = fs::remove_file(path);
        result
    }

    #[test]
    fn sample_quality_rejects_silence_short_audio_and_clipping() {
        assert_eq!(
            quality_for(&vec![0.0; 48_000], 48_000).code,
            Some("WAKE_SAMPLE_SILENT")
        );
        assert_eq!(
            quality_for(&vec![0.1; 1_000], 48_000).code,
            Some("WAKE_SAMPLE_TOO_SHORT")
        );
        assert_eq!(
            quality_for(&vec![1.0; 48_000], 48_000).code,
            Some("WAKE_SAMPLE_CLIPPED")
        );
        assert!(quality_for(&vec![0.1; 48_000], 48_000).accepted);
    }

    #[test]
    fn voice_activation_defaults_to_disabled_and_unconfigured() {
        assert_eq!(
            serde_json::to_value(VoiceActivationSettings::default()).unwrap(),
            serde_json::json!({
                "enabled": false,
                "setupCompleted": false,
                "referenceExists": false,
                "sensitivity": "standard",
                "preferPersonal": false,
                "defaultAvailable": false,
                "modelSource": null
            })
        );
    }

    #[test]
    fn fresh_install_can_use_default_without_enrollment_but_requires_opt_in() {
        let settings = reconcile_models(VoiceActivationSettings::default(), false, true);
        assert_eq!(settings.model_source, Some(WakeModelSource::Default));
        assert!(settings.reference_exists && settings.default_available);
        assert!(!settings.setup_completed && !settings.enabled);
    }

    #[test]
    fn switching_to_default_preserves_personal_enrollment_and_disabled_choice() {
        let settings = VoiceActivationSettings {
            setup_completed: true,
            ..VoiceActivationSettings::default()
        };
        let mut selected = reconcile_models(settings, true, true);
        assert_eq!(selected.model_source, Some(WakeModelSource::Default));
        assert!(selected.setup_completed);
        assert!(!selected.enabled);
        selected.prefer_personal = true;
        assert_eq!(
            reconcile_models(selected, true, true).model_source,
            Some(WakeModelSource::Personal)
        );
    }

    #[test]
    fn absent_models_disable_listening_and_unverified_personal_models_are_not_selected() {
        let settings = VoiceActivationSettings {
            enabled: true,
            ..VoiceActivationSettings::default()
        };
        for personal in [false, true] {
            let unavailable = reconcile_models(settings.clone(), personal, false);
            assert!(!unavailable.enabled && !unavailable.reference_exists);
            assert_eq!(unavailable.model_source, None);
        }
        let fallback = reconcile_models(settings, true, true);
        assert!(fallback.enabled);
        assert_eq!(fallback.model_source, Some(WakeModelSource::Default));
    }

    #[test]
    fn missing_default_keeps_a_verified_personal_model_usable() {
        let settings = VoiceActivationSettings {
            enabled: true,
            setup_completed: true,
            ..VoiceActivationSettings::default()
        };
        let fallback = reconcile_models(settings, true, false);
        assert!(fallback.enabled);
        assert_eq!(fallback.model_source, Some(WakeModelSource::Personal));
    }

    #[cfg(windows)]
    #[test]
    fn supplied_default_model_can_be_registered_by_the_live_engine() {
        if DEFAULT_REFERENCE.is_empty() {
            return; // CI has no private model; the selection policy is still tested above.
        }
        use rustpotter::WakewordLoad;
        let reference = rustpotter::WakewordRef::load_from_buffer(DEFAULT_REFERENCE).unwrap();
        assert!(!reference.samples_features.is_empty());
        let mut engine =
            rustpotter::Rustpotter::new(&rustpotter::RustpotterConfig::default()).unwrap();
        engine.add_wakeword_ref("ACE", reference).unwrap();
    }
}
