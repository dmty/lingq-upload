//! Pure-Rust windowed-RMS silence detector.
//!
//! Operates on a `PcmFrame` iterator (interleaved f32 samples). Each window
//! is `window_ms` wide (default: 30 ms); a silence run begins when all
//! windows in a `min_ms`-length span fall below `threshold_db`.

use crate::core::audio::SilenceRun;

use super::{PcmFrame, StreamInfo};

/// Default RMS window width in milliseconds.
const WINDOW_MS: u32 = 30;

/// Detect silence runs in a PCM stream.
///
/// - `stream` — iterator of interleaved f32 frames.
/// - `info`   — stream metadata (sample rate, channels).
/// - `threshold_db` — RMS level below which a window is "silent" (e.g. `-45.0`).
/// - `min_ms` — minimum consecutive silent duration to emit a `SilenceRun`.
///
/// Returns silence runs in ascending order of `start_ms`.
pub fn detect_silence<S>(
    stream: S,
    info: &StreamInfo,
    threshold_db: f32,
    min_ms: u32,
) -> Vec<SilenceRun>
where
    S: Iterator<Item = PcmFrame>,
{
    let sr = info.sample_rate as usize;
    let ch = info.channels as usize;
    let window_samples = (sr * WINDOW_MS as usize / 1000).max(1); // mono frames per window

    // Linear amplitude threshold (threshold_db is negative, e.g. -45.0).
    let linear_thresh = 10_f32.powf(threshold_db / 20.0);

    let mut runs: Vec<SilenceRun> = Vec::new();
    let mut buf: Vec<f32> = Vec::with_capacity(window_samples); // mono samples
    let mut sample_pos: usize = 0; // mono frame index
    let mut silence_start: Option<usize> = None; // start of current silent span (mono frames)

    let samples_per_ms = sr as f64 / 1000.0;

    for frame in stream {
        // Down-mix interleaved channels to mono by averaging.
        let ch_safe = ch.max(1);
        for i in 0..frame.frames {
            let offset = i * ch_safe;
            let sum: f32 = if ch_safe == 1 {
                frame.samples.get(offset).copied().unwrap_or(0.0)
            } else {
                frame.samples[offset..offset + ch_safe].iter().sum::<f32>()
            };
            buf.push(sum / ch_safe as f32);

            if buf.len() >= window_samples {
                // Compute RMS of this window.
                let rms = rms_f32(&buf);
                let is_silent = rms <= linear_thresh;
                let win_start = sample_pos;
                sample_pos += buf.len();
                buf.clear();

                if is_silent {
                    if silence_start.is_none() {
                        silence_start = Some(win_start);
                    }
                } else if let Some(ss) = silence_start.take() {
                    let start_ms = (ss as f64 / samples_per_ms).round() as u32;
                    let end_ms = (win_start as f64 / samples_per_ms).round() as u32;
                    if end_ms.saturating_sub(start_ms) >= min_ms {
                        runs.push(SilenceRun { start_ms, end_ms });
                    }
                }
            }
        }
    }

    // Flush any remaining partial window.
    if !buf.is_empty() {
        let rms = rms_f32(&buf);
        let is_silent = rms <= linear_thresh;
        let win_start = sample_pos;
        sample_pos += buf.len();
        if is_silent && silence_start.is_none() {
            silence_start = Some(win_start);
        } else if !is_silent {
            if let Some(ss) = silence_start.take() {
                let start_ms = (ss as f64 / samples_per_ms).round() as u32;
                let end_ms = (win_start as f64 / samples_per_ms).round() as u32;
                if end_ms.saturating_sub(start_ms) >= min_ms {
                    runs.push(SilenceRun { start_ms, end_ms });
                }
            }
        }
    }

    // Close any run that extends to end-of-stream.
    if let Some(ss) = silence_start {
        let start_ms = (ss as f64 / samples_per_ms).round() as u32;
        let end_ms = (sample_pos as f64 / samples_per_ms).round() as u32;
        if end_ms.saturating_sub(start_ms) >= min_ms {
            runs.push(SilenceRun { start_ms, end_ms });
        }
    }

    runs
}

fn rms_f32(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(sample_rate: u32) -> StreamInfo {
        StreamInfo {
            sample_rate,
            channels: 1,
            duration_sec: 0.0,
            codec: "wav",
        }
    }

    fn sine_frame(freq: f32, sample_rate: u32, n: usize) -> PcmFrame {
        let samples: Vec<f32> = (0..n)
            .map(|i| {
                0.5 * (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate as f32).sin()
            })
            .collect();
        PcmFrame { frames: n, samples }
    }

    fn silence_frame(n: usize) -> PcmFrame {
        PcmFrame {
            frames: n,
            samples: vec![0.0; n],
        }
    }

    /// Synthetic: [1 s sine][1 s silence][1 s sine] at 16 kHz.
    #[test]
    fn detects_one_second_silence_between_speech() {
        let sr = 16_000u32;
        let frames_1s = sr as usize;
        let frames: Vec<PcmFrame> = vec![
            sine_frame(440.0, sr, frames_1s),
            silence_frame(frames_1s),
            sine_frame(440.0, sr, frames_1s),
        ];
        let nfo = info(sr);
        let runs = detect_silence(frames.into_iter(), &nfo, -45.0, 500);
        assert_eq!(runs.len(), 1, "expected one silence run, got {runs:?}");
        let run = &runs[0];
        assert!(
            run.start_ms >= 950 && run.start_ms <= 1050,
            "start_ms {} not near 1000",
            run.start_ms
        );
        assert!(
            run.end_ms >= 1950 && run.end_ms <= 2050,
            "end_ms {} not near 2000",
            run.end_ms
        );
    }

    /// Min-duration gate: silence shorter than min_ms must be suppressed.
    #[test]
    fn short_silence_below_min_ms_suppressed() {
        let sr = 16_000u32;
        let frames: Vec<PcmFrame> = vec![
            sine_frame(440.0, sr, sr as usize),
            silence_frame(200 * sr as usize / 1000), // 200 ms — below 500 ms gate
            sine_frame(440.0, sr, sr as usize),
        ];
        let runs = detect_silence(frames.into_iter(), &info(sr), -45.0, 500);
        assert!(
            runs.is_empty(),
            "short silence should be suppressed: {runs:?}"
        );
    }

    /// Corpus calibration test: load the real WAV fixtures and assert ±50 ms.
    #[cfg(test)]
    mod corpus {
        use super::*;
        use crate::codecs::symphonia_impl::SymphoniaDecoder;
        use crate::codecs::AudioDecoder;

        const THRESHOLD_DB: f32 = -45.0;
        const MIN_MS: u32 = 500;
        const TOL_MS: u32 = 50;

        fn fixtures_dir() -> std::path::PathBuf {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/audio/silence_corpus")
        }

        fn decode_wav(path: &std::path::Path) -> (Vec<PcmFrame>, StreamInfo) {
            let mut dec = SymphoniaDecoder::open(path).expect("open wav");
            let info = dec.info();
            let mut frames = Vec::new();
            while let Some(f) = dec.next_frame().expect("frame") {
                frames.push(f);
            }
            (frames, info)
        }

        fn run_corpus_clip(
            stem: &str,
            expected_silence_runs: &[(u32, u32)], // (start_ms, end_ms) per run
        ) {
            let path = fixtures_dir().join(format!("{stem}.wav"));
            if !path.exists() {
                eprintln!("corpus fixture missing, skipping: {}", path.display());
                return;
            }
            let (frames, nfo) = decode_wav(&path);
            let runs = detect_silence(frames.into_iter(), &nfo, THRESHOLD_DB, MIN_MS);
            assert_eq!(
                runs.len(),
                expected_silence_runs.len(),
                "{stem}: wrong number of silence runs; got {runs:?}, expected {expected_silence_runs:?}"
            );
            for (i, (run, &(exp_start, exp_end))) in
                runs.iter().zip(expected_silence_runs.iter()).enumerate()
            {
                let start_diff = run.start_ms.abs_diff(exp_start);
                let end_diff = run.end_ms.abs_diff(exp_end);
                assert!(
                    start_diff <= TOL_MS,
                    "{stem} run[{i}] start_ms {} vs expected {exp_start} (diff {start_diff} > {TOL_MS})",
                    run.start_ms
                );
                assert!(
                    end_diff <= TOL_MS,
                    "{stem} run[{i}] end_ms {} vs expected {exp_end} (diff {end_diff} > {TOL_MS})",
                    run.end_ms
                );
            }
        }

        #[test]
        fn corpus_clip_a() {
            // Silence runs derived from golden: backward=5056,9024 forward=6016,10048
            run_corpus_clip("clip_a", &[(5056, 6016), (9024, 10048)]);
        }

        #[test]
        fn corpus_clip_b() {
            run_corpus_clip("clip_b", &[(4032, 5504), (7040, 8512)]);
        }

        #[test]
        fn corpus_clip_c() {
            // clip_c ends mid-silence; the last run closes at stream end.
            run_corpus_clip("clip_c", &[(3008, 4032), (7040, 9000)]);
        }
    }
}
