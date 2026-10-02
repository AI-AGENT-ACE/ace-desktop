use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{BufWriter, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, Runtime};
use uuid::Uuid;

const WAV_HEADER_SIZE: u64 = 44;
const MAX_RECORDING_SECONDS: u64 = 30 * 60;
const MAX_CHUNK_SAMPLES: usize = 32_768;
const MAX_UPLOAD_ATTEMPTS: u8 = 3;
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const MIN_RECORDING_DURATION_MS: u64 = 250;

fn voice_log(message: impl std::fmt::Display) {
    if cfg!(debug_assertions) {
        eprintln!("[Voice] {message}");
    }
}

#[derive(Default)]
pub struct VoiceRecordingState {
    recording: Mutex<Option<WavRecording>>,
    ready: Mutex<HashMap<String, ReadyRecording>>,
    uploading: Mutex<HashSet<String>>,
}
struct WavRecording {
    recording_id: String,
    writer: BufWriter<File>,
    temporary_path: PathBuf,
    saved_path: PathBuf,
    sample_rate: u32,
    channels: u16,
    samples_written: u64,
}
#[derive(Clone)]
struct ReadyRecording {
    path: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRecordingStart {
    recording_id: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRecordingResult {
    recording_id: String,
    state: &'static str,
    sample_rate: u32,
    channels: u16,
    samples_written: u64,
    duration_ms: u64,
    file_size: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceUploadResult {
    recording_id: String,
    state: &'static str,
    result: Value,
}

impl WavRecording {
    fn create(
        recording_id: String,
        saved_path: PathBuf,
        sample_rate: u32,
        channels: u16,
    ) -> Result<Self, String> {
        let temporary_path = saved_path.with_extension("wav.part");
        let mut writer =
            BufWriter::new(File::create(&temporary_path).map_err(|_| "RECORDING_FAILED")?);
        writer
            .write_all(&wav_header(sample_rate, channels, 0))
            .map_err(|_| "RECORDING_FAILED")?;
        Ok(Self {
            recording_id,
            writer,
            temporary_path,
            saved_path,
            sample_rate,
            channels,
            samples_written: 0,
        })
    }
    fn append(&mut self, samples: &[f32]) -> Result<(), String> {
        let maximum = self.sample_rate as u64 * self.channels as u64 * MAX_RECORDING_SECONDS;
        if self.samples_written + samples.len() as u64 > maximum {
            return Err("RECORDING_FAILED".into());
        }
        for sample in samples {
            let pcm = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
            self.writer
                .write_all(&pcm.to_le_bytes())
                .map_err(|_| "RECORDING_FAILED")?;
        }
        self.samples_written += samples.len() as u64;
        Ok(())
    }
    fn finish(mut self) -> Result<(VoiceRecordingResult, ReadyRecording), String> {
        if self.samples_written == 0 {
            drop(self.writer);
            let _ = fs::remove_file(&self.temporary_path);
            return Err("VOICE_RECORDING_EMPTY".into());
        }
        let duration_ms = (self.samples_written / self.channels as u64).saturating_mul(1000)
            / self.sample_rate as u64;
        if duration_ms < MIN_RECORDING_DURATION_MS {
            drop(self.writer);
            let _ = fs::remove_file(&self.temporary_path);
            return Err("VOICE_RECORDING_TOO_SHORT".into());
        }
        let data_size = self
            .samples_written
            .checked_mul(2)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or("WAV_FINALIZE_FAILED")?;
        self.writer.flush().map_err(|_| "WAV_FINALIZE_FAILED")?;
        self.writer
            .seek(SeekFrom::Start(0))
            .map_err(|_| "WAV_FINALIZE_FAILED")?;
        self.writer
            .write_all(&wav_header(self.sample_rate, self.channels, data_size))
            .map_err(|_| "WAV_FINALIZE_FAILED")?;
        self.writer.flush().map_err(|_| "WAV_FINALIZE_FAILED")?;
        drop(self.writer);
        fs::rename(&self.temporary_path, &self.saved_path).map_err(|_| "WAV_FINALIZE_FAILED")?;
        let file_size = fs::metadata(&self.saved_path)
            .map_err(|_| "WAV_FINALIZE_FAILED")?
            .len();
        let ready = ReadyRecording {
            path: self.saved_path,
        };
        Ok((
            VoiceRecordingResult {
                recording_id: self.recording_id,
                state: "READY",
                sample_rate: self.sample_rate,
                channels: self.channels,
                samples_written: self.samples_written,
                duration_ms,
                file_size,
            },
            ready,
        ))
    }
}

fn wav_header(sample_rate: u32, channels: u16, data_size: u32) -> [u8; WAV_HEADER_SIZE as usize] {
    let mut h = [0; WAV_HEADER_SIZE as usize];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36u32.saturating_add(data_size)).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(
        &sample_rate
            .saturating_mul(channels as u32)
            .saturating_mul(2)
            .to_le_bytes(),
    );
    h[32..34].copy_from_slice(&channels.saturating_mul(2).to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_size.to_le_bytes());
    h
}
fn voice_temp_dir() -> PathBuf {
    std::env::temp_dir().join("ace").join("voice")
}
fn configured_duration(name: &str, fallback: u64, min: u64, max: u64) -> Duration {
    Duration::from_secs(
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(fallback)
            .clamp(min, max),
    )
}
fn cleanup_expired_at(
    directory: &Path,
    now: SystemTime,
    maximum_age: Duration,
) -> Result<usize, String> {
    fs::create_dir_all(directory).map_err(|_| "RECORDING_FAILED")?;
    let mut removed = 0;
    for entry in fs::read_dir(directory)
        .map_err(|_| "RECORDING_FAILED")?
        .flatten()
    {
        let path = entry.path();
        let valid = path.file_name().and_then(|v| v.to_str()).is_some_and(|v| {
            v.starts_with("voice_") && (v.ends_with(".wav") || v.ends_with(".wav.part"))
        });
        if !valid {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(UNIX_EPOCH);
        if now.duration_since(modified).unwrap_or_default() > maximum_age
            && fs::remove_file(path).is_ok()
        {
            removed += 1;
        }
    }
    Ok(removed)
}
pub fn cleanup_expired<R: Runtime>(_: &tauri::AppHandle<R>) -> Result<usize, String> {
    cleanup_expired_at(
        &voice_temp_dir(),
        SystemTime::now(),
        configured_duration("ACE_VOICE_TEMP_TTL_SECONDS", 3600, 1800, 86400),
    )
}
fn require_voice_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "voice" {
        Ok(())
    } else {
        Err("BLOCKED".into())
    }
}
fn valid_sample_chunk(samples: &[f32]) -> bool {
    !samples.is_empty()
        && samples.len() <= MAX_CHUNK_SAMPLES
        && samples.iter().all(|v| v.is_finite())
}
fn retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

#[tauri::command]
pub fn start_voice_recording(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    sample_rate: u32,
    channels: u16,
) -> Result<VoiceRecordingStart, String> {
    require_voice_window(&window)?;
    if !(8_000..=192_000).contains(&sample_rate) || !(1..=2).contains(&channels) {
        return Err("RECORDING_FAILED".into());
    }
    let directory = voice_temp_dir();
    fs::create_dir_all(&directory).map_err(|_| "RECORDING_FAILED")?;
    let _ = cleanup_expired(&app);
    let state = app.state::<VoiceRecordingState>();
    let mut active = state.recording.lock().map_err(|_| "RECORDING_FAILED")?;
    if active.is_some() {
        return Err("DUPLICATE_VOICE_SESSION".into());
    }
    let id = format!("voice_{}", Uuid::new_v4());
    *active = Some(WavRecording::create(
        id.clone(),
        directory.join(format!("{id}.wav")),
        sample_rate,
        channels,
    )?);
    voice_log(format!(
        "recording started: id={id}, sample_rate={sample_rate}, channels={channels}"
    ));
    Ok(VoiceRecordingStart { recording_id: id })
}
#[tauri::command]
pub fn append_voice_recording_samples(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    samples: Vec<f32>,
) -> Result<(), String> {
    require_voice_window(&window)?;
    if !valid_sample_chunk(&samples) {
        return Err("RECORDING_FAILED".into());
    }
    app.state::<VoiceRecordingState>()
        .recording
        .lock()
        .map_err(|_| "RECORDING_FAILED")?
        .as_mut()
        .ok_or("RECORDING_FAILED".into())
        .and_then(|v| v.append(&samples))
}
#[tauri::command]
pub fn stop_voice_recording(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<VoiceRecordingResult, String> {
    require_voice_window(&window)?;
    let state = app.state::<VoiceRecordingState>();
    voice_log("manual stop requested");
    let recording = state
        .recording
        .lock()
        .map_err(|_| "WAV_FINALIZE_FAILED")?
        .take()
        .ok_or("RECORDING_FAILED")?;
    let id = recording.recording_id.clone();
    voice_log(format!(
        "recording finalizing: samples={}",
        recording.samples_written
    ));
    match recording.finish() {
        Ok((result, ready)) => {
            voice_log(format!(
                "recording finalized: id={}, samples={}, duration_ms={}, file_size={}",
                result.recording_id, result.samples_written, result.duration_ms, result.file_size
            ));
            let maximum = std::env::var("ACE_VOICE_MAX_FILE_SIZE")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(20 * 1024 * 1024)
                .clamp(1024, 32 * 1024 * 1024);
            if result.file_size > maximum {
                let _ = fs::remove_file(ready.path);
                return Err("VOICE_FILE_TOO_LARGE".into());
            }
            state
                .ready
                .lock()
                .map_err(|_| "RECORDING_FAILED")?
                .insert(id, ready);
            Ok(result)
        }
        Err(error) => {
            voice_log(format!("recording finalize failed: code={error}"));
            Err(error)
        }
    }
}
fn remove_active(recording: WavRecording) -> Result<(), String> {
    drop(recording.writer);
    for path in [recording.temporary_path, recording.saved_path] {
        if path.exists() {
            fs::remove_file(path).map_err(|_| "RECORDING_FAILED")?;
        }
    }
    Ok(())
}
#[tauri::command]
pub fn cancel_voice_recording(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    require_voice_window(&window)?;
    if let Some(v) = app
        .state::<VoiceRecordingState>()
        .recording
        .lock()
        .map_err(|_| "RECORDING_FAILED")?
        .take()
    {
        remove_active(v)?
    }
    Ok(())
}
#[tauri::command]
pub fn discard_voice_recording(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    recording_id: String,
) -> Result<(), String> {
    require_voice_window(&window)?;
    let ready = app
        .state::<VoiceRecordingState>()
        .ready
        .lock()
        .map_err(|_| "RECORDING_FAILED")?
        .remove(&recording_id)
        .ok_or("RECORDING_FAILED")?;
    fs::remove_file(ready.path).map_err(|_| "RECORDING_FAILED".to_owned())
}

#[tauri::command]
pub async fn upload_voice_recording(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    recording_id: String,
    api_base_url: String,
    access_token: String,
    conversation_id: Option<String>,
) -> Result<VoiceUploadResult, String> {
    require_voice_window(&window)?;
    if access_token.is_empty() || access_token.len() > 4096 {
        return Err("VOICE_UPLOAD_FAILED".into());
    }
    let mut endpoint = url::Url::parse(&api_base_url).map_err(|_| "VOICE_UPLOAD_FAILED")?;
    if !matches!(endpoint.scheme(), "http" | "https")
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
    {
        return Err("VOICE_UPLOAD_FAILED".into());
    }
    endpoint.set_path("voice/commands");
    endpoint.set_query(None);
    endpoint.set_fragment(None);
    let state = app.state::<VoiceRecordingState>();
    {
        let mut set = state.uploading.lock().map_err(|_| "VOICE_UPLOAD_FAILED")?;
        if !set.insert(recording_id.clone()) {
            return Err("DUPLICATE_VOICE_SESSION".into());
        }
    }
    let ready = state
        .ready
        .lock()
        .map_err(|_| "VOICE_UPLOAD_FAILED")?
        .get(&recording_id)
        .cloned()
        .ok_or("VOICE_FILE_INVALID")?;
    let client = reqwest::Client::builder()
        .connect_timeout(configured_duration(
            "ACE_VOICE_UPLOAD_TIMEOUT_SECONDS",
            15,
            1,
            120,
        ))
        .timeout(configured_duration(
            "ACE_VOICE_PROCESSING_TIMEOUT_SECONDS",
            60,
            1,
            300,
        ))
        .build()
        .map_err(|_| "VOICE_UPLOAD_FAILED")?;
    voice_log(format!("transcription started: id={recording_id}"));
    let mut outcome = Err("VOICE_UPLOAD_FAILED".to_owned());
    for attempt in 1..=MAX_UPLOAD_ATTEMPTS {
        let bytes = match tokio::fs::read(&ready.path).await {
            Ok(v) => v,
            Err(_) => {
                outcome = Err("VOICE_FILE_INVALID".into());
                break;
            }
        };
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(format!("{recording_id}.wav"))
            .mime_str("audio/wav")
            .map_err(|_| "VOICE_UPLOAD_FAILED")?;
        let mut form = reqwest::multipart::Form::new()
            .part("audio", part)
            .text("recordingId", recording_id.clone());
        if let Some(id) = &conversation_id {
            form = form.text("conversationId", id.clone())
        }
        let response = client
            .post(endpoint.clone())
            .bearer_auth(&access_token)
            .multipart(form)
            .send()
            .await;
        match response {
            Ok(v) if v.status().is_success() => match v.bytes().await {
                Ok(bytes) if bytes.len() <= MAX_RESPONSE_BYTES => {
                    match serde_json::from_slice::<Value>(&bytes) {
                        Ok(result) => {
                            outcome = Ok(VoiceUploadResult {
                                recording_id: recording_id.clone(),
                                state: "SUCCESS",
                                result,
                            });
                            break;
                        }
                        Err(_) => outcome = Err("VOICE_PROCESSING_FAILED".into()),
                    }
                }
                _ => outcome = Err("VOICE_PROCESSING_FAILED".into()),
            },
            Ok(v) if v.status().as_u16() == 401 => {
                outcome = Err("UNAUTHENTICATED".into());
                break;
            }
            Ok(v) => {
                outcome = Err("VOICE_PROCESSING_FAILED".into());
                if !retryable_status(v.status()) {
                    break;
                }
            }
            Err(e) if e.is_timeout() => outcome = Err("VOICE_TIMEOUT".into()),
            Err(_) => outcome = Err("VOICE_UPLOAD_FAILED".into()),
        }
        if attempt < MAX_UPLOAD_ATTEMPTS {
            tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await
        }
    }
    state
        .uploading
        .lock()
        .map_err(|_| "VOICE_UPLOAD_FAILED")?
        .remove(&recording_id);
    state
        .ready
        .lock()
        .map_err(|_| "VOICE_UPLOAD_FAILED")?
        .remove(&recording_id);
    let _ = fs::remove_file(ready.path);
    match &outcome {
        Ok(_) => voice_log(format!("transcription completed: id={recording_id}")),
        Err(code) => voice_log(format!(
            "transcription failed: id={recording_id}, code={code}"
        )),
    }
    outcome
}
pub fn cancel_active<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    if let Some(v) = app
        .state::<VoiceRecordingState>()
        .recording
        .lock()
        .map_err(|_| "RECORDING_FAILED")?
        .take()
    {
        remove_active(v)?
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{name}-{}.wav", Uuid::new_v4()))
    }
    #[test]
    fn writes_wav_and_returns_id_without_path() {
        let file = path("ace-wav");
        let mut r = WavRecording::create("voice_test".into(), file.clone(), 48_000, 1).unwrap();
        r.append(&vec![0.25; 12_000]).unwrap();
        let (result, _) = r.finish().unwrap();
        let b = fs::read(&file).unwrap();
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(&b[8..12], b"WAVE");
        assert_eq!(result.recording_id, "voice_test");
        assert_eq!(result.duration_ms, 250);
        assert_eq!(result.file_size, 24_044);
        fs::remove_file(file).unwrap();
    }
    #[test]
    fn empty_and_short_manual_recordings_return_specific_errors() {
        let empty = path("ace-empty");
        let recording =
            WavRecording::create("voice_empty".into(), empty.clone(), 48_000, 1).unwrap();
        assert!(matches!(recording.finish(), Err(code) if code == "VOICE_RECORDING_EMPTY"));
        assert!(!empty.with_extension("wav.part").exists());

        let short = path("ace-short");
        let mut recording =
            WavRecording::create("voice_short".into(), short.clone(), 48_000, 1).unwrap();
        recording.append(&vec![0.25; 4_800]).unwrap();
        assert!(matches!(recording.finish(), Err(code) if code == "VOICE_RECORDING_TOO_SHORT"));
        assert!(!short.with_extension("wav.part").exists());
    }
    #[test]
    fn cancel_removes_partial() {
        let file = path("ace-cancel");
        let r = WavRecording::create("voice_test".into(), file.clone(), 16_000, 1).unwrap();
        let partial = r.temporary_path.clone();
        remove_active(r).unwrap();
        assert!(!partial.exists());
        assert!(!file.exists());
    }
    #[test]
    fn rejects_bad_samples_and_long_recording() {
        assert!(!valid_sample_chunk(&[]));
        assert!(!valid_sample_chunk(&[f32::NAN]));
        let file = path("ace-limit");
        let mut r = WavRecording::create("voice_test".into(), file, 8_000, 1).unwrap();
        r.samples_written = 8_000 * MAX_RECORDING_SECONDS;
        assert!(r.append(&[0.0]).is_err());
        remove_active(r).unwrap();
    }
    #[test]
    fn startup_cleanup_removes_only_expired_voice_files() {
        let directory = std::env::temp_dir().join(format!("ace-cleanup-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let old = directory.join("voice_old.wav");
        let keep = directory.join("unrelated.txt");
        fs::write(&old, b"x").unwrap();
        fs::write(&keep, b"x").unwrap();
        assert_eq!(
            cleanup_expired_at(
                &directory,
                SystemTime::now() + Duration::from_secs(10),
                Duration::from_secs(1)
            )
            .unwrap(),
            1
        );
        assert!(keep.exists());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn retries_only_transient_http_failures() {
        assert!(retryable_status(reqwest::StatusCode::REQUEST_TIMEOUT));
        assert!(retryable_status(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(retryable_status(reqwest::StatusCode::BAD_GATEWAY));
        assert!(!retryable_status(reqwest::StatusCode::BAD_REQUEST));
        assert!(!retryable_status(reqwest::StatusCode::UNAUTHORIZED));
    }
}
