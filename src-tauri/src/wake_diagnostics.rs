//! Opt-in development capture. All PCM collection and disk writes run on the KWS worker.
use crate::{wake_diagnostic_data::Report, wake_kws::InputSample};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Instant,
};
use tauri::Manager;

static OPERATION: Mutex<()> = Mutex::new(());
static REQUEST: Mutex<Option<(PathBuf, String)>> = Mutex::new(None);
static STOP: AtomicBool = AtomicBool::new(false);
static STATUS: Mutex<Option<serde_json::Value>> = Mutex::new(None);
static RETAIN_SETUP: AtomicBool = AtomicBool::new(false);
thread_local! { static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) }; }
struct Capture {
    directory: PathBuf,
    label: String,
    started: Instant,
    rate: usize,
    float: bool,
    bytes: Vec<u8>,
    model: Vec<u8>,
    config: String,
    report: Report,
}
fn status(value: serde_json::Value) {
    *STATUS.lock().unwrap() = Some(value);
}
pub fn active() -> bool {
    CAPTURE.with(|c| c.borrow().is_some())
}
pub fn busy() -> bool {
    STATUS.lock().unwrap().as_ref().is_some_and(|status| {
        matches!(
            status["state"].as_str(),
            Some("starting" | "recording" | "saving")
        )
    })
}
pub fn initialize(config: &rustpotter::RustpotterConfig, model: &[u8]) {
    if let Some((directory, label)) = REQUEST.lock().unwrap().take() {
        let float = matches!(config.fmt.sample_format, rustpotter::SampleFormat::F32);
        CAPTURE.with(|c| {
            *c.borrow_mut() = Some(Capture {
                directory,
                label,
                started: Instant::now(),
                rate: config.fmt.sample_rate,
                float,
                bytes: Vec::with_capacity(config.fmt.sample_rate * 30 * if float { 4 } else { 2 }),
                model: model.to_vec(),
                config: format!("{config:?}"),
                report: Report::default(),
            })
        });
        status(serde_json::json!({"state":"recording", "maxSeconds":30}));
    }
}
pub fn before_frame<T: InputSample>(frame: &[T]) {
    let would_exceed = CAPTURE.with(|c| {
        c.borrow().as_ref().is_some_and(|c| {
            c.bytes.len() / if c.float { 4 } else { 2 } + frame.len() > c.rate * 30
        })
    });
    if would_exceed {
        finish("sample_limit");
        return;
    }
    let would_exceed = CAPTURE.with(|c| {
        c.borrow().as_ref().is_some_and(|c| {
            c.bytes.len() / if c.float { 4 } else { 2 } + frame.len() > c.rate * 30
        })
    });
    if would_exceed {
        finish("sample_limit");
        return;
    }
    CAPTURE.with(|c| {
        if let Some(c) = c.borrow_mut().as_mut() {
            for sample in frame {
                sample.append_bytes(&mut c.bytes);
            }
            rustpotter::diagnostics::begin_frame();
        }
    });
}
pub fn after_frame(spotter: &rustpotter::Rustpotter, detected: bool) {
    CAPTURE.with(|c| {
        if let Some(c) = c.borrow_mut().as_mut() {
            c.report
                .observe(spotter, detected, spotter.get_samples_per_frame(), c.rate);
        }
    });
    tick();
}
pub fn tick() {
    let reason = CAPTURE.with(|c| {
        c.borrow().as_ref().and_then(|c| {
            if STOP.load(Ordering::Relaxed) {
                Some("manual_stop")
            } else if c.started.elapsed().as_secs() >= 30 {
                Some("time_limit")
            } else if c.bytes.len() / if c.float { 4 } else { 2 } >= c.rate * 30 {
                Some("sample_limit")
            } else {
                None
            }
        })
    });
    if let Some(reason) = reason {
        finish(reason);
    }
}
pub fn finish(reason: &str) {
    let capture = CAPTURE.with(|c| c.borrow_mut().take());
    if let Some(capture) = capture {
        status(serde_json::json!({"state":"saving"}));
        match save(capture, reason) {
            Ok(value) => {
                eprintln!("[ACE Wake Diagnostic] saved {}", value["summary"]);
                status(value);
            }
            Err(error) => status(serde_json::json!({"state":"error", "error":error})),
        }
    }
}
fn save(c: Capture, reason: &str) -> Result<serde_json::Value, String> {
    std::fs::create_dir_all(&c.directory).map_err(|e| e.to_string())?;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: c.rate as u32,
        bits_per_sample: if c.float { 32 } else { 16 },
        sample_format: if c.float {
            hound::SampleFormat::Float
        } else {
            hound::SampleFormat::Int
        },
    };
    let wav = c.directory.join("input.wav");
    let mut writer = hound::WavWriter::create(&wav, spec).map_err(|e| e.to_string())?;
    if c.float {
        for b in c.bytes.as_chunks::<4>().0 {
            writer
                .write_sample(f32::from_le_bytes(*b))
                .map_err(|e| e.to_string())?;
        }
    } else {
        for b in c.bytes.as_chunks::<2>().0 {
            writer
                .write_sample(i16::from_le_bytes(*b))
                .map_err(|e| e.to_string())?;
        }
    }
    writer.finalize().map_err(|e| e.to_string())?;
    std::fs::write(c.directory.join("model.rpw"), &c.model).map_err(|e| e.to_string())?;
    let manifest = serde_json::json!({"schema":1, "label":c.label, "reason":reason,
        "model_sha256":format!("{:x}", Sha256::digest(&c.model)), "pcm_sha256":format!("{:x}", Sha256::digest(&c.bytes)),
        "engine_config":c.config, "initial_state":"fresh_listener", "orb_activation":"suppressed_during_capture",
        "quality":crate::wake_diagnostic_data::quality(&wav)?, "summary":c.report.summary()});
    std::fs::write(
        c.directory.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        c.directory.join("live-trace.json"),
        serde_json::to_vec(&c.report).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(
        serde_json::json!({"state":"saved", "directory":c.directory.to_string_lossy(), "summary":c.report.summary()}),
    )
}
fn require_main(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("BLOCKED".into());
    }
    Ok(())
}
#[tauri::command]
pub async fn start_wake_diagnostic(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    label: String,
) -> Result<(), String> {
    require_main(&window)?;
    if !matches!(label.as_str(), "positive" | "silence" | "negative") {
        return Err("Invalid label".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = OPERATION.lock().unwrap();
        if STATUS.lock().unwrap().as_ref().is_some_and(|s| {
            matches!(
                s["state"].as_str(),
                Some("recording" | "starting" | "saving")
            )
        }) {
            return Err("Diagnostic already active".into());
        }
        if !matches!(
            crate::wake_word::diagnostic_listener_state(&app),
            crate::wake_word::WakeWordState::Listening
        ) {
            return Err("Wake Word가 듣는 중일 때 시작해 주세요.".into());
        }
        crate::wake_word::stop(&app);
        STOP.store(false, Ordering::Relaxed);
        let directory = app
            .path()
            .app_local_data_dir()
            .map_err(|e| e.to_string())?
            .join("wake-diagnostics")
            .join(uuid::Uuid::new_v4().to_string());
        *REQUEST.lock().unwrap() = Some((directory, label));
        status(serde_json::json!({"state":"starting"}));
        if let Err(error) = crate::wake_word::restore_persisted(&app) {
            REQUEST.lock().unwrap().take();
            status(serde_json::json!({"state":"error", "error":error}));
            return Err(error);
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn stop_wake_diagnostic(window: tauri::WebviewWindow) -> Result<(), String> {
    require_main(&window)?;
    STOP.store(true, Ordering::Relaxed);
    Ok(())
}
#[tauri::command]
pub fn get_wake_diagnostic(window: tauri::WebviewWindow) -> Result<serde_json::Value, String> {
    require_main(&window)?;
    Ok(STATUS
        .lock()
        .unwrap()
        .clone()
        .unwrap_or(serde_json::json!({"state":"idle"})))
}
#[tauri::command]
pub fn retain_wake_setup_diagnostics(
    window: tauri::WebviewWindow,
    enabled: bool,
) -> Result<(), String> {
    require_main(&window)?;
    RETAIN_SETUP.store(enabled, Ordering::Relaxed);
    Ok(())
}
pub fn retain_sample(app: &tauri::AppHandle, path: &std::path::Path) -> Result<(), String> {
    if !RETAIN_SETUP.load(Ordering::Relaxed) {
        return Ok(());
    }
    let directory = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("wake-diagnostics")
        .join("registration");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let name = format!("{}", uuid::Uuid::new_v4());
    std::fs::copy(path, directory.join(format!("{name}.wav"))).map_err(|e| e.to_string())?;
    let quality = serde_json::json!({"kind":if path.file_name().is_some_and(|n| n == "test.wav") {"test"} else {"registration"},
        "sample_name":path.file_name().map(|n| n.to_string_lossy()),
        "captured_at_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_millis(),
        "quality":crate::wake_diagnostic_data::quality(path)?});
    std::fs::write(
        directory.join(format!("{name}.json")),
        serde_json::to_vec_pretty(&quality).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    eprintln!("[ACE Wake Diagnostic] registration quality: {quality}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpotter::{WakewordLoad, WakewordRefBuildFromFiles, WakewordSave};

    #[test]
    #[ignore = "Requires private reference WAV fixtures; intentionally excluded from publication"]
    fn captured_pcm_and_instrumented_results_match_original_engine() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let sample = root.join("resources/wake-word/samples/ace_1.wav");
        let reader = hound::WavReader::open(&sample).unwrap();
        let rate = reader.spec().sample_rate as usize;
        let source = reader
            .into_samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let reference = rustpotter::WakewordRef::new_from_sample_files(
            "ACE".into(),
            Some(0.52),
            Some(0.22),
            vec![sample.to_string_lossy().into_owned()],
            13,
        )
        .unwrap();
        let model = reference.save_to_buffer().unwrap();
        let config = crate::wake_kws::config(rate, rustpotter::SampleFormat::I16);
        let build = || {
            let mut engine = rustpotter::Rustpotter::new(&config).unwrap();
            engine
                .add_wakeword_ref(
                    "ACE",
                    rustpotter::WakewordRef::load_from_buffer(&model).unwrap(),
                )
                .unwrap();
            engine
        };
        let mut original = build();
        let mut instrumented = build();
        let mut samples = vec![0_i16; rate * 3];
        samples.extend(source);
        samples.extend(vec![0_i16; rate * 3]);
        let frame_size = original.get_samples_per_frame();
        samples.truncate(samples.len() / frame_size * frame_size);
        let directory = root.join("target/wake-diagnostic-fixture");
        *REQUEST.lock().unwrap() = Some((
            directory.clone(),
            "synthetic_fixture_not_user_microphone".into(),
        ));
        STOP.store(false, Ordering::Relaxed);
        initialize(&config, &model);
        let mut original_finals = 0;
        for frame in samples.chunks_exact(frame_size) {
            let expected = original.process_samples(frame.to_vec());
            before_frame(frame);
            let actual = instrumented.process_samples(frame.to_vec());
            after_frame(&instrumented, actual.is_some());
            assert_eq!(actual.is_some(), expected.is_some());
            if let (Some(a), Some(b)) = (actual, expected) {
                assert_eq!(a.score, b.score);
                original_finals += 1;
            }
        }
        assert!(original_finals > 0);
        STOP.store(true, Ordering::Relaxed);
        tick();
        assert!(!active());
        assert_eq!(STATUS.lock().unwrap().as_ref().unwrap()["state"], "saved");
        let restored = hound::WavReader::open(directory.join("input.wav"))
            .unwrap()
            .into_samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(restored, samples);
        let trace: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("live-trace.json")).unwrap())
                .unwrap();
        assert!(trace["buffering"].as_u64().unwrap() > 0);
        assert!(trace["candidate_matches"].as_u64().unwrap() > 0);
        assert!(
            trace["average_rejected"].as_u64().unwrap()
                + trace["reference_rejected"].as_u64().unwrap()
                > 0
        );
        assert_eq!(
            trace["frames"].as_u64().unwrap() as usize,
            samples.len() / frame_size
        );

        // Replay the same fixture as 48 kHz F32, the user's current live input format.
        let float_rate = 48_000;
        let floating: Vec<f32> = (0..samples.len() * float_rate / rate)
            .map(|i| {
                let position = i as f64 * rate as f64 / float_rate as f64;
                let left = position.floor() as usize;
                let right = (left + 1).min(samples.len() - 1);
                let fraction = (position - left as f64) as f32;
                (samples[left] as f32 * (1.0 - fraction) + samples[right] as f32 * fraction)
                    / 32767.0
            })
            .collect();
        let config = crate::wake_kws::config(float_rate, rustpotter::SampleFormat::F32);
        let mut engine = rustpotter::Rustpotter::new(&config).unwrap();
        engine
            .add_wakeword_ref(
                "ACE",
                rustpotter::WakewordRef::load_from_buffer(&model).unwrap(),
            )
            .unwrap();
        let directory = root.join("target/wake-diagnostic-f32-replay-fixture");
        *REQUEST.lock().unwrap() = Some((directory, "synthetic_f32_replay".into()));
        STOP.store(false, Ordering::Relaxed);
        initialize(&config, &model);
        for frame in floating.chunks_exact(engine.get_samples_per_frame()) {
            before_frame(frame);
            let actual = engine.process_samples(frame.to_vec());
            after_frame(&engine, actual.is_some());
        }
        STOP.store(true, Ordering::Relaxed);
        tick();
        assert_eq!(STATUS.lock().unwrap().as_ref().unwrap()["state"], "saved");

        // F32 samples are stored bit-for-bit; enforce the limit without real-time waiting.
        let directory = root.join("target/wake-diagnostic-f32-fixture");
        let config = crate::wake_kws::config(16_000, rustpotter::SampleFormat::F32);
        *REQUEST.lock().unwrap() = Some((directory.clone(), "synthetic_limit".into()));
        STOP.store(false, Ordering::Relaxed);
        initialize(&config, &model);
        let values = vec![0.125_f32; 16_000 * 30];
        before_frame(&values);
        tick();
        assert!(!active());
        let restored = hound::WavReader::open(directory.join("input.wav"))
            .unwrap()
            .into_samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(restored, values);
        assert_eq!(restored.len(), 16_000 * 30);
        rustpotter::diagnostics::take_frame();
        STOP.store(false, Ordering::Relaxed);
    }
}
