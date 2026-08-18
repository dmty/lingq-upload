# Silence detection

Status: implemented.

The carver detects silent runs in source audio to choose chapter cut points.
The runtime uses a windowed root-mean-square (RMS) pass over the decoded PCM
stream from `codecs::AudioDecoder`. Implementation lives in
`src-tauri/src/codecs/silence.rs::detect_silence`.

## Algorithm

- Window: 30 ms, non-overlapping — calibrated to keep the corpus regression
  within ±50 ms of the golden offsets.
- Mono-mix on the fly. Multichannel input is averaged per frame before the
  square-and-accumulate step.
- Threshold: caller-supplied dBFS, converted to a linear amplitude
  (`10^(db/20)`) and squared for comparison against the running mean square.
- Emit a `SilenceRun { start_ms, end_ms }` when a below-threshold span meets
  the caller-supplied `min_ms`.

## Calibration

The corpus regression in `codecs::silence::tests` (against
`tests/fixtures/audio/silence_corpus/`) pins detected edges to within ±50 ms
of the committed golden offsets. `CarveOpts::silence_db` in
`core/audio/carver.rs` holds the calibrated default; changing it changes
which sub-noise-floor passages register as silence — keep it co-located with
the golden offsets to stay auditable.
