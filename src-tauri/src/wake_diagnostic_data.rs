//! Development data only: aggregate every frame, preserve the actual rejection trace.
use rustpotter::{diagnostics::FrameTrace, Rustpotter};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Default, Serialize)]
pub struct Report {
    pub frames: u64,
    pub partial_active_frames: u64,
    pub candidate_matches: u64,
    pub final_detections: u64,
    pub buffering: u64,
    pub vad_skipped: u64,
    pub average_rejected: u64,
    pub reference_rejected: u64,
    pub min_scores_rejected: u64,
    pub waiting_countdown: u64,
    pub max_average: Option<f32>,
    pub max_score: Option<f32>,
    pub reference_maxima: BTreeMap<String, f32>,
    pub detection_ms: Vec<u64>,
    pub trace: Vec<serde_json::Value>,
}
impl Report {
    pub fn observe(
        &mut self,
        spotter: &Rustpotter,
        detected: bool,
        frame_samples: usize,
        rate: usize,
    ) {
        let trace: FrameTrace = rustpotter::diagnostics::take_frame();
        self.frames += 1;
        let at_ms = self.frames * frame_samples as u64 * 1000 / rate as u64;
        self.partial_active_frames += u64::from(spotter.get_partial_detection().is_some());
        self.candidate_matches += trace.candidate_matches as u64;
        self.final_detections += u64::from(detected);
        self.buffering += trace.buffering as u64;
        self.vad_skipped += trace.vad_skipped as u64;
        self.min_scores_rejected += trace.min_scores_rejected as u64;
        self.waiting_countdown += trace.waiting_countdown as u64;
        if detected {
            self.detection_ms.push(at_ms);
        }
        for comparison in &trace.comparisons {
            match comparison.outcome {
                "average_below_threshold_reference_not_computed" => self.average_rejected += 1,
                "reference_score_below_threshold" => self.reference_rejected += 1,
                _ => (),
            }
            if let Some(score) = comparison.average {
                self.max_average = Some(self.max_average.unwrap_or(score).max(score));
            }
            if let Some(score) = comparison.score {
                self.max_score = Some(self.max_score.unwrap_or(score).max(score));
            }
            for (name, score) in &comparison.references {
                self.reference_maxima
                    .entry(name.clone())
                    .and_modify(|max| *max = max.max(*score))
                    .or_insert(*score);
            }
        }
        self.trace.push(serde_json::json!({"at_ms": at_ms, "partial_active": spotter.get_partial_detection().is_some(), "final_detection": detected, "stages": trace}));
    }
    pub fn summary(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(self).unwrap();
        value.as_object_mut().unwrap().remove("trace");
        value
    }
}

pub fn quality(path: &std::path::Path) -> Result<serde_json::Value, String> {
    let reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let samples: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => reader
            .into_samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?,
        (hound::SampleFormat::Int, 16) => reader
            .into_samples::<i16>()
            .map(|v| v.map(|v| v as f32 / 32768.0))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?,
        _ => return Err("Expected PCM I16 or F32".into()),
    };
    let rate = spec.sample_rate as f64;
    let mono: Vec<f32> = samples
        .chunks_exact(spec.channels as usize)
        .map(|s| s.iter().sum::<f32>() / spec.channels as f32)
        .collect();
    // Amplitude-based estimate, not a speech/VAD classifier. 10 ms RMS windows.
    let window = (spec.sample_rate / 100).max(1) as usize;
    let active: Vec<_> = mono
        .chunks(window)
        .enumerate()
        .filter(|(_, s)| rms(s) >= 0.01)
        .map(|(i, _)| i)
        .collect();
    let first = active.first().map(|i| i * window);
    let end = active.last().map(|i| ((i + 1) * window).min(mono.len()));
    Ok(serde_json::json!({
        "sample_rate": spec.sample_rate, "channels": spec.channels, "format": format!("{:?}/{}", spec.sample_format, spec.bits_per_sample),
        "duration_ms": mono.len() as f64 * 1000.0 / rate, "rms": rms(&mono),
        "peak": mono.iter().fold(0.0_f32, |a, v| a.max(v.abs())),
        "clipped_samples": mono.iter().filter(|v| v.abs() >= 0.999).count(),
        "leading_quiet_ms": first.map(|n| n as f64 * 1000.0 / rate),
        "trailing_quiet_ms": end.map(|n| (mono.len() - n) as f64 * 1000.0 / rate),
        "active_span_ms": first.zip(end).map(|(a,b)| (b-a) as f64 * 1000.0 / rate),
        "active_window_count": active.len(), "activity_rule": "10ms RMS >= 0.01; estimate, not VAD; null when no active windows"
    }))
}
fn rms(samples: &[f32]) -> f64 {
    (samples.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / samples.len().max(1) as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpotter::WakewordRefBuildFromFiles;
    #[test]
    #[ignore = "Requires private reference WAV fixtures; intentionally excluded from publication"]
    fn countdown_reports_min_scores_rejection() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/wake-word/samples/ace_1.wav");
        let reader = hound::WavReader::open(&path).unwrap();
        let rate = reader.spec().sample_rate as usize;
        let mut samples = reader
            .into_samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        samples.extend(vec![0_i16; rate * 6]);
        let model = rustpotter::WakewordRef::new_from_sample_files(
            "ACE".into(),
            Some(0.52),
            Some(0.22),
            vec![path.to_string_lossy().into_owned()],
            13,
        )
        .unwrap();
        let mut config = crate::wake_kws::config(rate, rustpotter::SampleFormat::I16);
        config.detector.min_scores = usize::MAX; // Test-only impossible finalization gate.
        let mut engine = rustpotter::Rustpotter::new(&config).unwrap();
        engine.add_wakeword_ref("ACE", model).unwrap();
        let frame_size = engine.get_samples_per_frame();
        let mut report = Report::default();
        for frame in samples.chunks_exact(frame_size) {
            rustpotter::diagnostics::begin_frame();
            let detection = engine.process_samples(frame.to_vec());
            report.observe(&engine, detection.is_some(), frame_size, rate);
        }
        assert!(report.candidate_matches > 0);
        assert!(report.waiting_countdown > 0);
        assert!(report.min_scores_rejected > 0);
        assert_eq!(report.final_detections, 0);
    }
    #[test]
    #[ignore = "Requires private reference WAV fixtures; intentionally excluded from publication"]
    fn actual_comparator_distinguishes_average_gate_from_reference_gate() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for average_gate in [true, false] {
            let mut model = rustpotter::WakewordRef::new_from_sample_files(
                "ACE".into(),
                Some(0.52),
                Some(0.22),
                vec![
                    root.join("resources/wake-word/samples/ace_0.wav")
                        .to_string_lossy()
                        .into_owned(),
                    root.join("resources/wake-word/samples/ace_1.wav")
                        .to_string_lossy()
                        .into_owned(),
                ],
                13,
            )
            .unwrap();
            assert!(model.avg_features.is_some());
            // Impossible test-only gates force each rejection path; never changes the active model.
            model.avg_threshold = Some(if average_gate { 2.0 } else { 0.0 });
            model.threshold = Some(2.0);
            let config = crate::wake_kws::config(16_000, rustpotter::SampleFormat::F32);
            let mut engine = rustpotter::Rustpotter::new(&config).unwrap();
            engine.add_wakeword_ref("ACE", model).unwrap();
            let frame_size = engine.get_samples_per_frame();
            let mut report = Report::default();
            for _ in 0..160 {
                rustpotter::diagnostics::begin_frame();
                let detection = engine.process_samples(vec![0.0_f32; frame_size]);
                report.observe(&engine, detection.is_some(), frame_size, 16_000);
            }
            assert_eq!(report.final_detections, 0);
            assert!(report.buffering > 0);
            if average_gate {
                assert!(report.average_rejected > 0);
                assert!(report.max_score.is_none());
                assert!(report.reference_maxima.is_empty());
            } else {
                assert!(report.reference_rejected > 0);
                assert!(report.max_score.is_some());
                assert!(report.max_average.is_none());
            }
        }
    }
}
