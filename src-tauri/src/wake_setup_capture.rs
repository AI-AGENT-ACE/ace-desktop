//! Registration uses the same native device, configuration and mono conversion as listening.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub struct Captured {
    pub samples: Vec<f32>,
    pub detected: bool,
}

pub struct Capture {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<Result<Captured, String>>>,
}

impl Capture {
    pub fn start(reference: Option<PathBuf>) -> Result<(Self, u32, String), String> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let result = run(stopping, reference, &ready_tx);
            if let Err(error) = &result {
                let _ = ready_tx.try_send(Err(error.clone()));
            }
            result
        });
        let capture = Self {
            stop,
            worker: Some(worker),
        };
        match ready_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok((rate, identity))) => Ok((capture, rate, identity)),
            Ok(Err(error)) => Err(error),
            Err(_) => Err("WAKE_MIC_START_TIMEOUT".into()),
        }
    }

    pub fn finish(mut self) -> Result<Captured, String> {
        self.stop.store(true, Ordering::Release);
        self.worker
            .take()
            .unwrap()
            .join()
            .map_err(|_| "WAKE_CAPTURE_THREAD_FAILED")?
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

type Ready = mpsc::SyncSender<Result<(u32, String), String>>;

fn run(
    stop: Arc<AtomicBool>,
    reference: Option<PathBuf>,
    ready: &Ready,
) -> Result<Captured, String> {
    let host = cpal::default_host();
    let device = select_device(&host)?;
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let rate = supported.sample_rate();
    let channels = supported.channels();
    let identity = format!(
        "{} / {rate}Hz / {channels}ch / {:?}",
        device.description().map_err(|e| e.to_string())?.name(),
        supported.sample_format()
    );
    let mut config: cpal::StreamConfig = supported.into();
    config.buffer_size = cpal::BufferSize::Default;
    let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(12);
    let failed = Arc::new(AtomicBool::new(false));
    let overflow = failed.clone();
    let stream_error = failed.clone();
    let on_error = move |error: cpal::Error| {
        crate::wake_word::wake_log(format!("setup stream error: {error}"));
        if error.kind() != cpal::ErrorKind::Xrun {
            stream_error.store(true, Ordering::Release);
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
                    overflow.store(true, Ordering::Release);
                }
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _| {
                let mono = crate::wake_word::downmix_i16(data, channels)
                    .into_iter()
                    .map(|s| f32::from(s) / 32767.0)
                    .collect();
                if tx.try_send(mono).is_err() {
                    overflow.store(true, Ordering::Release);
                }
            },
            on_error,
            None,
        ),
        _ => return Err("WAKE_MIC_UNSUPPORTED_FORMAT".into()),
    }
    .map_err(|e| e.to_string())?;
    let mut detector = reference
        .map(|path| {
            use rustpotter::WakewordLoad;
            let mut detector = rustpotter::Rustpotter::new(&crate::wake_kws::config(
                rate as usize,
                rustpotter::SampleFormat::F32,
            ))?;
            detector.add_wakeword_ref(
                "ACE",
                rustpotter::WakewordRef::load_from_file(&path.to_string_lossy())?,
            )?;
            Ok::<_, String>(detector)
        })
        .transpose()?;
    stream.play().map_err(|e| e.to_string())?;
    crate::wake_word::wake_log(format!(
        "native setup capture started: {identity}, live_test={}",
        detector.is_some()
    ));
    ready
        .send(Ok((rate, identity)))
        .map_err(|_| "WAKE_CAPTURE_CANCELLED")?;
    let mut captured = Captured {
        samples: Vec::new(),
        detected: false,
    };
    let mut pending = Vec::new();
    #[cfg(debug_assertions)]
    let mut gates = crate::wake_word::GateDiagnostics::default();
    let started = Instant::now();
    while !stop.load(Ordering::Acquire) {
        if started.elapsed() > Duration::from_secs(10) {
            return Err("WAKE_SAMPLE_TOO_LONG".into());
        }
        if failed.load(Ordering::Acquire) {
            return Err("WAKE_CAPTURE_AUDIO_LOST".into());
        }
        if let Ok(mono) = rx.recv_timeout(Duration::from_millis(20)) {
            if let Some(detector) = detector.as_mut() {
                crate::wake_kws::feed(
                    &mut pending,
                    &mono,
                    detector.get_samples_per_frame(),
                    |frame| {
                        #[cfg(debug_assertions)]
                        rustpotter::diagnostics::begin_frame();
                        captured.detected |= detector.process_samples(frame).is_some();
                        #[cfg(debug_assertions)]
                        gates.observe(rustpotter::diagnostics::take_frame());
                    },
                );
            }
            captured.samples.extend(mono);
            if captured.samples.len() > rate as usize * 10 {
                return Err("WAKE_SAMPLE_TOO_LONG".into());
            }
        }
    }
    drop(stream);
    if failed.load(Ordering::Acquire) {
        return Err("WAKE_CAPTURE_AUDIO_LOST".into());
    }
    crate::wake_word::wake_log(format!(
        "native setup capture finished: samples={}, detected={}",
        captured.samples.len(),
        captured.detected
    ));
    #[cfg(debug_assertions)]
    if detector.is_some() {
        crate::wake_word::wake_log(format!("native setup KWS gates: {gates:?}"));
    }
    Ok(captured)
}

pub fn select_device(host: &cpal::Host) -> Result<cpal::Device, String> {
    if let Some(device) = host.default_input_device() {
        return Ok(device);
    }
    let mut usable: Vec<_> = host
        .input_devices()
        .map_err(|e| e.to_string())?
        .filter(|device| {
            device.default_input_config().is_ok_and(|c| {
                matches!(
                    c.sample_format(),
                    cpal::SampleFormat::I16 | cpal::SampleFormat::F32
                )
            })
        })
        .collect();
    if usable.len() == 1 {
        Ok(usable.remove(0))
    } else {
        Err("WAKE_MIC_DEVICE_SELECTION_REQUIRED".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires a physical microphone; run explicitly on the target Windows machine"]
    fn native_capture_starts_stops_and_releases_device() {
        for _ in 0..2 {
            let (capture, rate, _) = Capture::start(None).expect("native microphone starts");
            std::thread::sleep(Duration::from_millis(350));
            let result = capture.finish().expect("native microphone stops");
            assert!(!result.samples.is_empty());
            assert!(result.samples.len() < rate as usize * 2);
            assert!(result.samples.iter().all(|sample| sample.is_finite()));
            assert!(
                !result.detected,
                "no model must never fabricate a detection"
            );
        }
    }
}
