//! Builds a candidate only. Never creates approval or evaluation records.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rustpotter::{WakewordRef, WakewordRefBuildFromFiles, WakewordSave};
    use sha2::{Digest, Sha256};
    let mut args = std::env::args_os().skip(1);
    let input = std::path::PathBuf::from(
        args.next()
            .ok_or("expected private training WAV directory")?,
    );
    let output = std::path::PathBuf::from(args.next().ok_or("expected new candidate .rpw path")?);
    if args.next().is_some() || output.exists() {
        return Err("Output must be a new file; never overwrite personal or public models".into());
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(input)? {
        let path = entry?.path();
        if path.extension().and_then(|v| v.to_str()) != Some("wav") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or("invalid WAV filename")?;
        if !name.starts_with('p')
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
        {
            return Err("Use anonymous unique filenames such as p01_01.wav".into());
        }
        let mut reader = hound::WavReader::open(&path)?;
        let spec = reader.spec();
        if spec.channels != 1
            || spec.sample_rate != 16000
            || spec.bits_per_sample != 16
            || spec.sample_format != hound::SampleFormat::Int
        {
            return Err("Expected mono 16 kHz PCM16 WAV".into());
        }
        let samples = reader.samples::<i16>().collect::<Result<Vec<_>, _>>()?;
        if !(5600..=56000).contains(&samples.len()) {
            return Err("Training clips must be 0.35..3.5 seconds".into());
        }
        let rms = (samples
            .iter()
            .map(|v| (*v as f64 / 32768.0).powi(2))
            .sum::<f64>()
            / samples.len() as f64)
            .sqrt();
        let clipped = samples.iter().filter(|v| v.unsigned_abs() >= 32760).count() as f64
            / samples.len() as f64;
        if rms < 0.008 || clipped > 0.02 {
            return Err("Reject silent or clipped training clips".into());
        }
        files.push(path.to_string_lossy().into_owned());
    }
    files.sort();
    if files.len() < 15 || files.len() > 100 {
        return Err("Use 15..100 consented training clips from multiple speakers".into());
    }
    let model =
        WakewordRef::new_from_sample_files("ACE".into(), Some(0.52), Some(0.22), files, 13)?;
    let bytes = model.save_to_buffer()?;
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    file.write_all(&bytes)?;
    println!("Candidate only: sizeBytes={} sha256={:x}. Consent and independent live evaluation still required.", bytes.len(), Sha256::digest(&bytes));
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("This project evaluates the public model on Windows");
}
