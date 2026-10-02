//! Replay one capture; optional single-condition gain or reference-threshold experiment.
#[cfg(all(windows, debug_assertions))]
#[path = "../src/wake_diagnostic_data.rs"]
mod wake_diagnostic_data;
#[cfg(all(windows, debug_assertions))]
#[allow(dead_code)] // InputSample byte collection is used by the live capture, not replay.
#[path = "../src/wake_kws.rs"]
mod wake_kws;

#[cfg(all(windows, debug_assertions))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rustpotter::{Rustpotter, WakewordLoad, WakewordRef, WakewordSave};
    use sha2::{Digest, Sha256};
    let mut args = std::env::args_os().skip(1);
    let directory = std::path::PathBuf::from(args.next().ok_or("Expected capture directory")?);
    let experiment = args.next();
    let experimental_threshold = experiment
        .as_ref()
        .and_then(|v| v.to_str())
        .and_then(|v| v.strip_prefix("--reference-threshold="))
        .map(str::parse::<f32>)
        .transpose()?;
    if experimental_threshold.is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        || (experiment.as_ref().is_some_and(|v| v != "--gain-off")
            && experimental_threshold.is_none())
        || args.next().is_some()
    {
        return Err("Use one condition: --gain-off or --reference-threshold=<0..1>".into());
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json"))?)?;
    let bytes = std::fs::read(directory.join("model.rpw"))?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if manifest["model_sha256"] != hash {
        return Err("Model checksum mismatch".into());
    }
    let mut model = WakewordRef::load_from_buffer(&bytes)?;
    if let Some(value) = experimental_threshold {
        model.threshold = Some(value);
    }
    let candidate_model = if experimental_threshold.is_some() {
        Some(model.save_to_buffer()?)
    } else {
        None
    };
    let max_frames = model
        .samples_features
        .values()
        .map(Vec::len)
        .max()
        .ok_or("Empty reference")?;
    println!(
        "model_sha256={hash} references={} model_threshold={:?} model_avg_threshold={:?}",
        model.samples_features.len(),
        model.threshold,
        model.avg_threshold
    );
    let reader = hound::WavReader::open(directory.join("input.wav"))?;
    let spec = reader.spec();
    println!(
        "quality={}",
        wake_diagnostic_data::quality(&directory.join("input.wav"))?
    );
    if spec.channels != 1 {
        return Err("Expected captured engine mono input".into());
    }
    let format = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => rustpotter::SampleFormat::F32,
        (hound::SampleFormat::Int, 16) => rustpotter::SampleFormat::I16,
        _ => return Err("Unsupported capture format".into()),
    };
    let mut config = wake_kws::config(spec.sample_rate as usize, format);
    if manifest["engine_config"] != format!("{config:?}") {
        return Err("Configuration drift: use the capture's source revision".into());
    }
    if experiment.as_ref().is_some_and(|v| v == "--gain-off") {
        config.filters.gain_normalizer.enabled = false;
    }
    println!("config={config:?}");
    let mut spotter = Rustpotter::new(&config)?;
    spotter.add_wakeword_ref("ACE", model)?;
    let tail_samples = (max_frames * 30 / 1000 + 3) * spec.sample_rate as usize;
    let mut report = wake_diagnostic_data::Report::default();
    fn run<T: rustpotter::Sample + Default + Copy>(
        samples: &[T],
        tail_samples: usize,
        spotter: &mut Rustpotter,
        report: &mut wake_diagnostic_data::Report,
        rate: usize,
    ) -> usize {
        let frame_size = spotter.get_samples_per_frame();
        let mut pending = Vec::new();
        let mut feed = |input: &[T]| {
            wake_kws::feed(&mut pending, input, frame_size, |frame| {
                rustpotter::diagnostics::begin_frame();
                let detection = spotter.process_samples(frame);
                report.observe(spotter, detection.is_some(), frame_size, rate);
            })
        };
        // Arbitrary transport chunks; shared feeder retains all incomplete frames.
        for chunk in samples.chunks(4096) {
            feed(chunk);
        }
        let input_frames = samples.len() / frame_size;
        feed(&vec![T::default(); tail_samples]);
        input_frames
    }
    let input_frames = if spec.sample_format == hound::SampleFormat::Float {
        let samples = reader
            .into_samples::<f32>()
            .collect::<Result<Vec<_>, _>>()?;
        let raw: Vec<_> = samples.iter().flat_map(|v| v.to_le_bytes()).collect();
        if manifest["pcm_sha256"] != format!("{:x}", Sha256::digest(&raw)) {
            return Err("PCM checksum mismatch".into());
        }
        run(
            &samples,
            tail_samples,
            &mut spotter,
            &mut report,
            spec.sample_rate as usize,
        )
    } else {
        let samples = reader
            .into_samples::<i16>()
            .collect::<Result<Vec<_>, _>>()?;
        let raw: Vec<_> = samples.iter().flat_map(|v| v.to_le_bytes()).collect();
        if manifest["pcm_sha256"] != format!("{:x}", Sha256::digest(&raw)) {
            return Err("PCM checksum mismatch".into());
        }
        run(
            &samples,
            tail_samples,
            &mut spotter,
            &mut report,
            spec.sample_rate as usize,
        )
    };
    let live: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("live-trace.json"))?)?;
    let live_frames = live["trace"].as_array().ok_or("Missing live trace")?;
    // Compare both traces after the same JSON serialization/deserialization path.
    // serde_json's default float parser can differ by one f64 ULP from an in-memory f32-to-f64 value.
    let replay_prefix: Vec<serde_json::Value> =
        serde_json::from_slice(&serde_json::to_vec(&report.trace[..input_frames])?)?;
    let same_prefix = input_frames == live_frames.len() && replay_prefix == live_frames[..];
    println!("input_frames={input_frames} tail_samples={tail_samples} live_prefix_equal={same_prefix} experiment={:?}", experiment);
    println!("summary={}", report.summary());
    let filename = if let Some(value) = experimental_threshold {
        format!("replay-threshold-{value}.json")
    } else if experiment.is_some() {
        "replay-gain-off.json".to_owned()
    } else {
        "replay-baseline.json".to_owned()
    };
    std::fs::write(
        directory.join(filename),
        serde_json::to_vec_pretty(
            &serde_json::json!({"source_model_sha256":hash, "reference_threshold_override":experimental_threshold, "summary":report.summary(), "live_prefix_equal":same_prefix, "input_frames":input_frames, "trace":report.trace}),
        )?,
    )?;
    if let (Some(value), Some(bytes)) = (experimental_threshold, candidate_model) {
        std::fs::write(
            directory.join(format!("model-threshold-{value}.rpw")),
            bytes,
        )?;
        println!("Candidate model exported locally; active model was not modified.");
    }
    if experiment.is_none() && !same_prefix {
        return Err(
            "Live/replay mismatch; do not use this capture for tuning before investigation".into(),
        );
    }
    Ok(())
}
#[cfg(not(all(windows, debug_assertions)))]
fn main() {
    eprintln!("Replay instrumentation is available in Windows debug builds only.");
}
