//! Read-only model metadata inspection; never prints paths or audio/features.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rustpotter::{WakewordLoad, WakewordRef};
    use sha2::{Digest, Sha256};
    let path = std::env::args_os()
        .nth(1)
        .ok_or("Expected reference file argument")?;
    let bytes = std::fs::read(path)?;
    let model = WakewordRef::load_from_buffer(&bytes)?;
    let mut frames: Vec<_> = model.samples_features.values().map(Vec::len).collect();
    frames.sort_unstable();
    println!("model_id={:x} references={} rms_level={:.6} mfcc_size={} threshold={:?} avg_threshold={:?} reference_frames={:?} average_frames={:?}",
        Sha256::digest(&bytes), frames.len(), model.rms_level, model.mfcc_size,
        model.threshold, model.avg_threshold, frames, model.avg_features.as_ref().map(Vec::len));
    if let Some(other_path) = std::env::args_os().nth(2) {
        let other = WakewordRef::load_from_buffer(&std::fs::read(other_path)?)?;
        let mut left = serde_json::to_value(&model)?;
        let mut right = serde_json::to_value(&other)?;
        left.as_object_mut()
            .ok_or("Invalid model")?
            .remove("threshold");
        right
            .as_object_mut()
            .ok_or("Invalid comparison model")?
            .remove("threshold");
        if left != right {
            return Err("Model fields other than threshold differ".into());
        }
        println!("all_fields_except_threshold_equal=true");
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This project enables Rustpotter on Windows only.");
}
