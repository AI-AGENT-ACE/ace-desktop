use serde::Serialize;
use std::{
    fs::{self, File},
    io::{BufWriter, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, Runtime};

const WAV_HEADER_SIZE: u64 = 44;
const MAX_RECORDING_SECONDS: u64 = 30 * 60;
const MAX_CHUNK_SAMPLES: usize = 32_768;

#[derive(Default)]
pub struct VoiceRecordingState {
    recording: Mutex<Option<WavRecording>>,
}

struct WavRecording {
    writer: BufWriter<File>,
    path: PathBuf,
    sample_rate: u32,
    channels: u16,
    samples_written: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRecordingResult {
    path: String,
    sample_rate: u32,
    channels: u16,
    samples_written: u64,
    duration_ms: u64,
}

impl WavRecording {
    fn create(path: PathBuf, sample_rate: u32, channels: u16) -> Result<Self, String> {
        let mut writer = BufWriter::new(File::create(&path).map_err(|error| error.to_string())?);
        writer
            .write_all(&wav_header(sample_rate, channels, 0))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            writer,
            path,
            sample_rate,
            channels,
            samples_written: 0,
        })
    }

    fn append(&mut self, samples: &[f32]) -> Result<(), String> {
        let max_samples = self.sample_rate as u64 * self.channels as u64 * MAX_RECORDING_SECONDS;
        if self.samples_written + samples.len() as u64 > max_samples {
            return Err("녹음은 최대 30분까지 저장할 수 있습니다.".into());
        }
        for sample in samples {
            let pcm = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
            self.writer
                .write_all(&pcm.to_le_bytes())
                .map_err(|error| error.to_string())?;
        }
        self.samples_written += samples.len() as u64;
        Ok(())
    }

    fn finish(mut self) -> Result<VoiceRecordingResult, String> {
        let data_size = self
            .samples_written
            .checked_mul(2)
            .and_then(|size| u32::try_from(size).ok())
            .ok_or_else(|| "녹음 파일 크기가 WAV 제한을 초과했습니다.".to_owned())?;
        self.writer.flush().map_err(|error| error.to_string())?;
        self.writer
            .seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        self.writer
            .write_all(&wav_header(self.sample_rate, self.channels, data_size))
            .map_err(|error| error.to_string())?;
        self.writer.flush().map_err(|error| error.to_string())?;

        let frames = self.samples_written / self.channels as u64;
        Ok(VoiceRecordingResult {
            path: self.path.to_string_lossy().into_owned(),
            sample_rate: self.sample_rate,
            channels: self.channels,
            samples_written: self.samples_written,
            duration_ms: frames.saturating_mul(1000) / self.sample_rate as u64,
        })
    }
}

fn wav_header(sample_rate: u32, channels: u16, data_size: u32) -> [u8; WAV_HEADER_SIZE as usize] {
    let mut header = [0_u8; WAV_HEADER_SIZE as usize];
    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&(36_u32.saturating_add(data_size)).to_le_bytes());
    header[8..12].copy_from_slice(b"WAVE");
    header[12..16].copy_from_slice(b"fmt ");
    header[16..20].copy_from_slice(&16_u32.to_le_bytes());
    header[20..22].copy_from_slice(&1_u16.to_le_bytes());
    header[22..24].copy_from_slice(&channels.to_le_bytes());
    header[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate
        .saturating_mul(channels as u32)
        .saturating_mul(2);
    header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    header[32..34].copy_from_slice(&(channels.saturating_mul(2)).to_le_bytes());
    header[34..36].copy_from_slice(&16_u16.to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&data_size.to_le_bytes());
    header
}

fn recordings_dir<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("recordings"))
        .map_err(|error| error.to_string())
}

fn recording_path(directory: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let base = directory.join(format!("ace-recording-{timestamp}.wav"));
    if !base.exists() {
        return base;
    }
    for suffix in 1..=999 {
        let candidate = directory.join(format!("ace-recording-{timestamp}-{suffix}.wav"));
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("ace-recording-{timestamp}-overflow.wav"))
}

fn require_voice_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "voice" {
        Ok(())
    } else {
        Err("BLOCKED".into())
    }
}

#[tauri::command]
pub fn start_voice_recording(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    sample_rate: u32,
    channels: u16,
) -> Result<(), String> {
    require_voice_window(&window)?;
    if !(8_000..=192_000).contains(&sample_rate) || !(1..=2).contains(&channels) {
        return Err("지원하지 않는 녹음 형식입니다.".into());
    }
    let directory = recordings_dir(&app)?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let state = app.state::<VoiceRecordingState>();
    let mut active = state
        .recording
        .lock()
        .map_err(|_| "녹음 상태를 잠글 수 없습니다.".to_owned())?;
    if active.is_some() {
        return Err("이미 녹음 중입니다.".into());
    }
    let recording = WavRecording::create(recording_path(&directory), sample_rate, channels)?;
    *active = Some(recording);
    Ok(())
}

#[tauri::command]
pub fn append_voice_recording_samples(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    samples: Vec<f32>,
) -> Result<(), String> {
    require_voice_window(&window)?;
    if samples.is_empty()
        || samples.len() > MAX_CHUNK_SAMPLES
        || samples.iter().any(|v| !v.is_finite())
    {
        return Err("잘못된 음성 샘플입니다.".into());
    }
    let state = app.state::<VoiceRecordingState>();
    let mut active = state
        .recording
        .lock()
        .map_err(|_| "녹음 상태를 잠글 수 없습니다.".to_owned())?;
    active
        .as_mut()
        .ok_or_else(|| "진행 중인 녹음이 없습니다.".to_owned())?
        .append(&samples)
}

#[tauri::command]
pub fn stop_voice_recording(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<VoiceRecordingResult, String> {
    require_voice_window(&window)?;
    finish_active(&app)?.ok_or_else(|| "진행 중인 녹음이 없습니다.".to_owned())
}

#[tauri::command]
pub fn cancel_voice_recording(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    require_voice_window(&window)?;
    let recording = app
        .state::<VoiceRecordingState>()
        .recording
        .lock()
        .map_err(|_| "녹음 상태를 잠글 수 없습니다.".to_owned())?
        .take();
    if let Some(recording) = recording {
        drop(recording.writer);
        fs::remove_file(recording.path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn finish_active<R: Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<Option<VoiceRecordingResult>, String> {
    let recording = app
        .state::<VoiceRecordingState>()
        .recording
        .lock()
        .map_err(|_| "녹음 상태를 잠글 수 없습니다.".to_owned())?
        .take();
    recording.map(WavRecording::finish).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn writes_playable_pcm_wav_header_and_samples() {
        let path = std::env::temp_dir().join(format!(
            "ace-wav-test-{}.wav",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut recording = WavRecording::create(path.clone(), 48_000, 1).unwrap();
        recording.append(&[-1.0, 0.0, 1.0]).unwrap();
        let result = recording.finish().unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 6);
        assert_eq!(bytes.len(), 50);
        assert_eq!(result.samples_written, 3);
        fs::remove_file(path).unwrap();
    }
}
