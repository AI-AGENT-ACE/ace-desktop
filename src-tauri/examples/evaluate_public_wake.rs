#[cfg(windows)]
#[path = "../src/wake_kws.rs"]
#[allow(dead_code)]
mod wake_kws;
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rustpotter::{Rustpotter, WakewordLoad, WakewordRef};
    let mut args = std::env::args_os().skip(1);
    let model = args.next().ok_or("expected candidate model path")?;
    let audio = args.next().ok_or("expected evaluation WAV path")?;
    if args.next().is_some() {
        return Err("expected model and WAV only".into());
    }
    let mut reader = hound::WavReader::open(audio)?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err("Expected mono PCM16 WAV".into());
    }
    let mut detector = Rustpotter::new(&wake_kws::config(
        spec.sample_rate as usize,
        rustpotter::SampleFormat::I16,
    ))?;
    detector.add_wakeword_ref(
        "ACE",
        WakewordRef::load_from_file(&model.to_string_lossy())?,
    )?;
    let frame_size = detector.get_samples_per_frame();
    let mut frame = Vec::with_capacity(frame_size);
    let mut detections = 0;
    let seconds = reader.duration() as f64 / spec.sample_rate as f64;
    for sample in reader.samples::<i16>() {
        frame.push(sample?);
        if frame.len() == frame_size
            && detector
                .process_samples(std::mem::take(&mut frame))
                .is_some()
        {
            detections += 1;
        }
    }
    println!("Replay only: durationSeconds={seconds:.3} detections={detections}. Live input and multi-speaker testing are separate.");
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows required");
}
