//! Shared live/replay configuration and frame boundaries.
pub trait InputSample: rustpotter::Sample {
    #[cfg(debug_assertions)]
    fn append_bytes(self, output: &mut Vec<u8>);
}
impl InputSample for i16 {
    #[cfg(debug_assertions)]
    fn append_bytes(self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.to_le_bytes());
    }
}
impl InputSample for f32 {
    #[cfg(debug_assertions)]
    fn append_bytes(self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.to_le_bytes());
    }
}
pub fn config(rate: usize, format: rustpotter::SampleFormat) -> rustpotter::RustpotterConfig {
    let mut config = rustpotter::RustpotterConfig::default();
    config.fmt.sample_rate = rate;
    config.fmt.channels = 1;
    config.fmt.sample_format = format;
    config.filters.gain_normalizer.enabled = true;
    config.detector.threshold = 0.52;
    config.detector.avg_threshold = 0.22;
    config.detector.min_scores = 2;
    config.detector.eager = false;
    config
}

pub fn feed<T: rustpotter::Sample>(
    pending: &mut Vec<T>,
    input: &[T],
    frame_size: usize,
    mut frame: impl FnMut(Vec<T>),
) {
    pending.extend_from_slice(input);
    while pending.len() >= frame_size {
        frame(pending.drain(..frame_size).collect());
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn chunk_boundaries_do_not_change_frames() {
        let mut pending = Vec::new();
        let mut frames = Vec::new();
        super::feed(&mut pending, &[1_i16, 2], 3, |f| frames.push(f));
        super::feed(&mut pending, &[3_i16, 4, 5, 6, 7], 3, |f| frames.push(f));
        assert_eq!(frames, vec![vec![1, 2, 3], vec![4, 5, 6]]);
        assert_eq!(pending, vec![7]);
    }
}
