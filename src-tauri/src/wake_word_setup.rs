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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VoiceActivationSettings {
    pub enabled: bool,
    pub setup_completed: bool,
    pub reference_exists: bool,
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
    let mut settings = settings_path(app)
        .ok()
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<VoiceActivationSettings>(&bytes).ok())
        .unwrap_or_default();
    let reference_valid = active_reference_path(app)
        .ok()
        .is_some_and(|path| validate_reference_file(&path));
    if !reference_valid {
        settings.enabled = false;
        settings.setup_completed = false;
        settings.reference_exists = false;
        let _ = save_settings(app, &settings);
    } else {
        settings.reference_exists = true;
    }
    settings
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
pub fn start_wake_word_sample(
    app: AppHandle,
    window: tauri::WebviewWindow,
    sample_rate: u32,
) -> Result<(), String> {
    require_main(&window)?;
    if !(8_000..=192_000).contains(&sample_rate) {
        return Err("WAKE_SAMPLE_FORMAT_INVALID".into());
    }
    let paths = sample_paths(&app)?;
    if paths.len() >= SAMPLE_TOTAL {
        return Err("WAKE_SAMPLE_LIMIT_REACHED".into());
    }
    let path = staging_dir(&app)?.join(format!("sample_{:02}.wav", paths.len() + 1));
    begin_recording(&app, path, sample_rate)
}

fn begin_recording(app: &AppHandle, path: PathBuf, sample_rate: u32) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let writer = hound::WavWriter::create(&path, spec)
        .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED".to_owned())?;
    let runtime = app.state::<WakeWordSetupRuntime>();
    let mut recording = runtime
        .recording
        .lock()
        .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?;
    if recording.is_some() {
        return Err("WAKE_SAMPLE_ALREADY_RECORDING".into());
    }
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
    sample_rate: u32,
) -> Result<(), String> {
    require_main(&window)?;
    if !(8_000..=192_000).contains(&sample_rate) || !candidate_reference_path(&app)?.is_file() {
        return Err("WAKE_TEST_NOT_READY".into());
    }
    begin_recording(&app, staging_dir(&app)?.join("test.wav"), sample_rate)
}

#[tauri::command]
pub fn append_wake_word_sample(
    app: AppHandle,
    window: tauri::WebviewWindow,
    samples: Vec<f32>,
) -> Result<(), String> {
    require_main(&window)?;
    if samples.is_empty()
        || samples.len() > 65_536
        || samples.iter().any(|value| !value.is_finite())
    {
        return Err("WAKE_SAMPLE_FORMAT_INVALID".into());
    }
    let runtime = app.state::<WakeWordSetupRuntime>();
    let mut guard = runtime
        .recording
        .lock()
        .map_err(|_| "WAKE_SAMPLE_RECORDING_FAILED")?;
    let recording = guard.as_mut().ok_or("WAKE_SAMPLE_NOT_RECORDING")?;
    for sample in samples {
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
    #[cfg(all(windows, debug_assertions))]
    if let Err(error) = crate::wake_diagnostics::retain_sample(&app, &recording.path) {
        crate::wake_word::wake_log(format!("diagnostic sample retention failed: {error}"));
    }
    if !result.accepted {
        let _ = fs::remove_file(recording.path);
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
    let test_path = staging_dir(&app)?.join("test.wav");
    if !test_path.is_file() {
        return Err("WAKE_TEST_NOT_READY".into());
    }
    #[cfg(windows)]
    {
        use rustpotter::WakewordLoad;
        let mut reader = hound::WavReader::open(test_path).map_err(|_| "WAKE_TEST_FAILED")?;
        let spec = reader.spec();
        let mut config = rustpotter::RustpotterConfig::default();
        config.fmt.sample_rate = spec.sample_rate as usize;
        config.fmt.channels = spec.channels;
        config.fmt.sample_format = rustpotter::SampleFormat::I16;
        config.filters.gain_normalizer.enabled = true;
        config.detector.threshold = 0.52;
        config.detector.avg_threshold = 0.22;
        config.detector.min_scores = 2;
        config.detector.eager = false;
        let mut detector = rustpotter::Rustpotter::new(&config).map_err(|_| "WAKE_TEST_FAILED")?;
        let reference = rustpotter::WakewordRef::load_from_file(
            &candidate_reference_path(&app)?.to_string_lossy(),
        )
        .map_err(|_| "WAKE_TEST_FAILED")?;
        let model = reference_metadata(&candidate_reference_path(&app)?)?;
        crate::wake_word::wake_log(format!(
            "onboarding test started: model_id={}, references={}, input_sample_rate={}, input_channels={}, input_format=I16, threshold={:.2}, avg_threshold={:.2}, min_scores={}, eager={}",
            model.id,
            model.reference_count,
            spec.sample_rate,
            spec.channels,
            config.detector.threshold,
            config.detector.avg_threshold,
            config.detector.min_scores,
            config.detector.eager,
        ));
        detector
            .add_wakeword_ref("ACE", reference)
            .map_err(|_| "WAKE_TEST_FAILED")?;
        let frame = detector.get_samples_per_frame();
        let mut samples = reader
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "WAKE_TEST_FAILED")?;
        samples.extend(std::iter::repeat_n(0, spec.sample_rate as usize * 3));
        let detected = samples
            .chunks_exact(frame)
            .any(|chunk| detector.process_samples(chunk.to_vec()).is_some());
        crate::wake_word::wake_log(format!(
            "onboarding test completed: model_id={}, detected={detected}, final_rms={:.6}, final_gain={:.4}",
            model.id,
            detector.get_rms_level(),
            detector.get_gain(),
        ));
        Ok(detected)
    }
    #[cfg(not(windows))]
    Err("WAKE_SETUP_UNSUPPORTED".into())
}

#[tauri::command]
pub fn complete_wake_word_setup(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<VoiceActivationSettings, String> {
    require_main(&window)?;
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
    };
    save_settings(&app, &settings)?;
    let _ = fs::remove_dir_all(staging_dir(&app)?);
    Ok(settings)
}

fn cancel_setup_files(app: &AppHandle) -> Result<(), String> {
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
    if enabled && !(settings.setup_completed && settings.reference_exists) {
        return Err("WAKE_SETUP_REQUIRED".into());
    }
    settings.enabled = enabled;
    save_settings(app, &settings)?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quality_for(samples: &[f32], rate: u32) -> SampleQuality {
        let mut recording = SetupRecording {
            writer: hound::WavWriter::create(
                std::env::temp_dir().join("ace-wake-quality-test.wav"),
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
        quality(&recording)
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
                "referenceExists": false
            })
        );
    }
}
