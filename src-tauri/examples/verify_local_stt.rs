#[path = "../src/local_stt.rs"]
mod local_stt;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::env::args_os()
        .nth(1)
        .ok_or("Expected a local WAV path")?;
    let mut reader = hound::WavReader::open(source)?;
    let spec = reader.spec();
    if spec.channels != 1 {
        return Err("Expected mono input".into());
    }
    let input = std::env::temp_dir().join(format!("ace-stt-check-{}.wav", uuid::Uuid::new_v4()));
    let mut writer = hound::WavWriter::create(
        &input,
        hound::WavSpec {
            channels: 1,
            sample_rate: spec.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )?;
    if spec.sample_format == hound::SampleFormat::Float {
        for sample in reader.samples::<f32>() {
            writer.write_sample((sample?.clamp(-1.0, 1.0) * 32767.0) as i16)?;
        }
    } else {
        for sample in reader.samples::<i16>() {
            writer.write_sample(sample?)?;
        }
    }
    writer.finalize()?;
    let started = std::time::Instant::now();
    let beam = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "5".into())
        .parse::<usize>()?;
    let result = if beam == 5 {
        local_stt::transcribe(&input)
    } else {
        local_stt::transcribe_with_beam(&input, beam)
    };
    let _ = std::fs::remove_file(&input);
    let result = result?;
    println!(
        "beam={beam} transcript_chars={} portfolio_requested={} elapsed_ms={}",
        result.transcript.chars().count(),
        result.portfolio_requested,
        started.elapsed().as_millis()
    );
    Ok(())
}
