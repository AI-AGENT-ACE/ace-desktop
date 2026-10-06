//! Local Korean transcription for a fixed, explicitly supported command.
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTranscript {
    pub transcript: String,
    pub portfolio_requested: bool,
}

pub fn portfolio_requested(text: &str) -> bool {
    let normalized: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '.' | ',' | '!' | '?' | '_'))
        .flat_map(char::to_lowercase)
        .collect();
    let text = normalized
        .strip_prefix("바탕화면에서")
        .or_else(|| normalized.strip_prefix("바탕화면에있는"))
        .unwrap_or(&normalized);
    let text = text.strip_prefix("김환성").unwrap_or(text);
    matches!(
        text,
        "포트폴리오열어줘"
            | "포트폴리오pdf열어줘"
            | "포트폴리오열어주세요"
            | "포트폴리오pdf열어주세요"
    )
}

struct TemporaryFiles(Vec<PathBuf>);
impl Drop for TemporaryFiles {
    fn drop(&mut self) {
        for file in &self.0 {
            let _ = fs::remove_file(file);
        }
    }
}

fn resample(samples: &[i16], rate: u32) -> Vec<i16> {
    let count = samples.len() as u64 * 16000 / u64::from(rate);
    (0..count)
        .map(|index| {
            let start = index as f64 * f64::from(rate) / 16000.0;
            let end = (index + 1) as f64 * f64::from(rate) / 16000.0;
            let mut total = 0.0;
            for (input, sample) in samples
                .iter()
                .enumerate()
                .take((end.ceil() as usize).min(samples.len()))
                .skip(start.floor() as usize)
            {
                let weight = end.min((input + 1) as f64) - start.max(input as f64);
                total += f64::from(*sample) * weight;
            }
            (total / (end - start)).round().clamp(-32768.0, 32767.0) as i16
        })
        .collect()
}

pub fn transcribe(path: &Path) -> Result<LocalTranscript, String> {
    let directory = std::env::var_os("ACE_LOCAL_STT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local/whisper"));
    let executable = directory.join("runtime/Release/whisper-cli.exe");
    let model = directory.join("ggml-small-q5_1.bin");
    if !executable.is_file() || !model.is_file() {
        return Err("LOCAL_STT_NOT_INSTALLED".into());
    }
    if fs::metadata(path).map_err(|_| "VOICE_FILE_INVALID")?.len() > 16 * 1024 * 1024 {
        return Err("VOICE_FILE_INVALID".into());
    }
    let reader = hound::WavReader::open(path).map_err(|_| "VOICE_FILE_INVALID")?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
        || !(8000..=96000).contains(&spec.sample_rate)
    {
        return Err("VOICE_FILE_INVALID".into());
    }
    let samples = reader
        .into_samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "VOICE_FILE_INVALID")?;
    if samples.len() < spec.sample_rate as usize / 4
        || samples.len() > spec.sample_rate as usize * 60
    {
        return Err("VOICE_RECORDING_TOO_SHORT".into());
    }
    let rms = (samples
        .iter()
        .map(|&s| (f64::from(s) / 32768.0).powi(2))
        .sum::<f64>()
        / samples.len() as f64)
        .sqrt();
    if rms < 0.001 {
        return Err("VOICE_RECORDING_EMPTY".into());
    }
    let wav = path.with_extension("local-stt.wav");
    let prefix = path.with_extension("local-stt");
    let json = path.with_extension("local-stt.json");
    let _cleanup = TemporaryFiles(vec![wav.clone(), json.clone()]);
    let mut writer = hound::WavWriter::create(
        &wav,
        hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .map_err(|_| "LOCAL_STT_FAILED")?;
    for sample in resample(&samples, spec.sample_rate) {
        writer
            .write_sample(sample)
            .map_err(|_| "LOCAL_STT_FAILED")?;
    }
    writer.finalize().map_err(|_| "LOCAL_STT_FAILED")?;
    let mut command = Command::new(executable);
    command
        .arg("-m")
        .arg(model)
        .arg("-f")
        .arg(&wav)
        .args(["-l", "ko", "-t", "4", "-ng", "-nf", "-nt", "-np", "-oj"])
        .arg("-of")
        .arg(&prefix)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().map_err(|_| "LOCAL_STT_FAILED")?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err("LOCAL_STT_FAILED".into());
                }
                break;
            }
            Ok(None) if started.elapsed() < Duration::from_secs(90) => {
                std::thread::sleep(Duration::from_millis(50))
            }
            outcome => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(if outcome.is_err() {
                    "LOCAL_STT_FAILED"
                } else {
                    "VOICE_TIMEOUT"
                }
                .into());
            }
        }
    }
    if fs::metadata(&json).map_err(|_| "LOCAL_STT_FAILED")?.len() > 1024 * 1024 {
        return Err("LOCAL_STT_FAILED".into());
    }
    let output: serde_json::Value =
        serde_json::from_slice(&fs::read(&json).map_err(|_| "LOCAL_STT_FAILED")?)
            .map_err(|_| "LOCAL_STT_FAILED")?;
    let segments = output["transcription"]
        .as_array()
        .ok_or("LOCAL_STT_FAILED")?;
    let transcript = segments
        .iter()
        .filter_map(|item| item["text"].as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned();
    Ok(LocalTranscript {
        portfolio_requested: portfolio_requested(&transcript),
        transcript,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_only_explicit_portfolio_open_commands() {
        for text in [
            "바탕화면에서 포트폴리오 열어줘",
            "바탕 화면에 있는 김환성 포트폴리오 PDF 열어 주세요.",
            "포트폴리오 열어줘!",
        ] {
            assert!(portfolio_requested(text), "{text}");
        }
        for text in [
            "",
            "포트폴리오 열지 마",
            "포트폴리오 삭제해줘",
            "포트폴리오 말고 이력서 열어줘",
            "포트폴리오 열어줘라고 말하지 않았어",
            "안녕",
        ] {
            assert!(!portfolio_requested(text), "{text}");
        }
    }
    #[test]
    fn conversion_preserves_duration_and_amplitude() {
        for rate in [16000, 44100, 48000] {
            let output = resample(&vec![1234; rate as usize], rate);
            assert_eq!(output.len(), 16000);
            assert!(output.iter().all(|&value| value == 1234));
        }
    }
}
