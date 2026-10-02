//! ACE development-only observer. Does not change scoring or detector decisions.
use serde::Serialize;
use std::cell::RefCell;

#[derive(Default, Serialize)]
pub struct FrameTrace {
    pub buffering: usize,
    pub vad_skipped: usize,
    pub candidate_matches: usize,
    pub min_scores_rejected: usize,
    pub finalized: usize,
    pub waiting_countdown: usize,
    pub comparisons: Vec<Comparison>,
}
#[derive(Serialize)]
pub struct Comparison {
    pub average: Option<f32>,
    pub average_threshold: f32,
    pub score: Option<f32>,
    pub threshold: f32,
    pub references: Vec<(String, f32)>,
    pub outcome: &'static str,
}
thread_local! { static TRACE: RefCell<Option<FrameTrace>> = const { RefCell::new(None) }; }
pub fn begin_frame() {
    TRACE.with(|t| *t.borrow_mut() = Some(FrameTrace::default()));
}
pub fn take_frame() -> FrameTrace {
    TRACE.with(|t| t.borrow_mut().take().unwrap_or_default())
}
pub fn observe(f: impl FnOnce(&mut FrameTrace)) {
    TRACE.with(|t| {
        if let Some(trace) = t.borrow_mut().as_mut() {
            f(trace);
        }
    });
}
