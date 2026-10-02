# ACE diagnostic patch (Rustpotter 3.0.2)

Source: locally installed crates.io release `rustpotter 3.0.2`.
Original Apache-2.0 license is retained in LICENSE. Cargo resolves this copy through
the application's `[patch.crates-io]`; the global Cargo registry is not modified.

Changes are confined to `src/diagnostics.rs`, its `lib.rs` export, and observer
calls in `src/detector.rs` and `src/wakewords/comp/wakeword_comp.rs`.

All instrumentation is `cfg(debug_assertions)`. A thread-local observer is only
enabled by `begin_frame()` during explicit capture/replay. Unobserved debug runs
only perform an empty thread-local check; release builds contain no observer.
No scoring formula, thresholds, countdown, reset, or acceptance conditions change.
The observer never recalculates a score the original comparator skipped.

- MFCC buffering / VAD skip: scoring has not run.
- `avg_score < avg_threshold`: reference scores have not been computed.
- `score <= threshold`: reference comparison ran but failed.
- Candidate match: actual comparator acceptance, possibly several per PCM frame.
- Pending countdown: finalization is waiting.
- Countdown completion with `counter < min_scores`: partial candidate rejected.
- Finalized: original detector returned a final detection.

Reference keys are sorted and replaced by ordinals to avoid logging paths.
These ordinals are meaningful only within the same model hash.
This patch does not add raw audio to logs. Opt-in PCM capture is in ACE's worker.
