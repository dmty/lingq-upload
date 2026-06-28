use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use super::AudioError;

/// How a silent chapter-divider is folded into its neighbour tracks.
///
/// `Forward` is the legacy behaviour and the default for newly created
/// projects: every cut lands at the END of the silent run, so the silence
/// glues onto the NEXT chapter. `Backward` cuts at the START of the silence
/// (silence stays with the PREVIOUS chapter). `Drop` emits a paired (start,
/// end) so the silent run is excised from both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "lowercase")]
pub enum AbsorbPolicy {
    #[default]
    Forward,
    Backward,
    Drop,
}

#[derive(Debug, Clone, Copy)]
pub struct CarveOpts {
    pub silence_db: f32,
    pub min_silence_ms: u32,
    pub absorb: AbsorbPolicy,
}

impl Default for CarveOpts {
    fn default() -> Self {
        Self {
            // Audiobook noise floors sit around -50 to -60 dB. -45 dB is loud
            // enough to ignore room tone and quiet enough to detect real gaps.
            silence_db: -45.0,
            min_silence_ms: 500,
            absorb: AbsorbPolicy::Forward,
        }
    }
}

/// Distinguishes the role a boundary plays for the caller.
///
/// `Cut` is a single split point (Forward / Backward modes — silence is glued
/// to one neighbour). `DropStart` / `DropEnd` arrive paired and share the same
/// `track_index`: the span `[DropStart, DropEnd]` is excluded from both tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryKind {
    Cut,
    DropStart,
    DropEnd,
}

/// A single cut point in the source audio. `track_index` is the 0-based index
/// of the chapter that BEGINS at this offset (so the first boundary's offset
/// is the start of track 1, not 0). `cut_offset_ms` is sample-accurate to
/// within ±50ms tolerance — see `clip_*.golden_offsets.json` for the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Boundary {
    pub track_index: usize,
    pub cut_offset_ms: u32,
    pub kind: BoundaryKind,
}

/// A detected run of silence in the source audio, in milliseconds from start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SilenceRun {
    pub start_ms: u32,
    pub end_ms: u32,
}

#[derive(Debug, Error)]
pub enum CarveError {
    #[error("audio: {0}")]
    Audio(#[from] AudioError),
}

/// Map detected silence runs into per-policy chapter boundaries.
///
/// Pure function — no IO.
///
/// `Forward`: cut at the END of each silence run (silence absorbed by NEXT).
/// `Backward`: cut at the START of each silence run (silence absorbed by PREV).
/// `Drop`: emit BOTH ends so the silent span belongs to no track.
pub fn boundaries_from_silences(runs: &[SilenceRun], policy: AbsorbPolicy) -> Vec<Boundary> {
    let mut out = Vec::with_capacity(runs.len() * 2);
    // `next_track` advances once per run, regardless of how many boundaries
    // the policy emits.
    for (next_track, run) in (1..).zip(runs.iter()) {
        let entries: &[(u32, BoundaryKind)] = match policy {
            AbsorbPolicy::Forward => &[(run.end_ms, BoundaryKind::Cut)],
            AbsorbPolicy::Backward => &[(run.start_ms, BoundaryKind::Cut)],
            AbsorbPolicy::Drop => &[
                (run.start_ms, BoundaryKind::DropStart),
                (run.end_ms, BoundaryKind::DropEnd),
            ],
        };
        for &(cut_offset_ms, kind) in entries {
            out.push(Boundary {
                track_index: next_track,
                cut_offset_ms,
                kind,
            });
        }
    }
    out
}

pub async fn carve(audio: &Path, opts: CarveOpts) -> Result<Vec<Boundary>, CarveError> {
    let runs = detect_silences(audio, opts.silence_db, opts.min_silence_ms).await?;
    Ok(boundaries_from_silences(&runs, opts.absorb))
}

async fn detect_silences(
    path: &Path,
    silence_db: f32,
    min_silence_ms: u32,
) -> Result<Vec<SilenceRun>, CarveError> {
    let path = path.to_path_buf();
    let runs = tokio::task::spawn_blocking(move || {
        use crate::codecs::silence::detect_silence;
        use crate::codecs::{symphonia_impl::SymphoniaDecoder, AudioDecoder};

        let mut dec = SymphoniaDecoder::open(&path).map_err(CarveError::Audio)?;
        let info = dec.info();
        let iter = std::iter::from_fn(move || match dec.next_frame() {
            Ok(Some(f)) => Some(f),
            _ => None,
        });
        Ok::<_, CarveError>(detect_silence(iter, &info, silence_db, min_silence_ms))
    })
    .await
    .map_err(|e| CarveError::Audio(AudioError::Io(e.to_string())))??;
    Ok(runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx() -> [SilenceRun; 2] {
        [
            SilenceRun {
                start_ms: 5_000,
                end_ms: 6_000,
            },
            SilenceRun {
                start_ms: 9_000,
                end_ms: 10_000,
            },
        ]
    }

    #[test]
    fn forward_cuts_at_silence_end() {
        let b = boundaries_from_silences(&fx(), AbsorbPolicy::Forward);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].cut_offset_ms, 6_000);
        assert_eq!(b[0].kind, BoundaryKind::Cut);
        assert_eq!(b[1].cut_offset_ms, 10_000);
        assert_eq!(b[1].kind, BoundaryKind::Cut);
    }

    #[test]
    fn backward_cuts_at_silence_start() {
        let b = boundaries_from_silences(&fx(), AbsorbPolicy::Backward);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].cut_offset_ms, 5_000);
        assert_eq!(b[0].kind, BoundaryKind::Cut);
        assert_eq!(b[1].cut_offset_ms, 9_000);
        assert_eq!(b[1].kind, BoundaryKind::Cut);
    }

    #[test]
    fn drop_emits_paired_cuts() {
        let b = boundaries_from_silences(&fx(), AbsorbPolicy::Drop);
        assert_eq!(b.len(), 4);
        assert_eq!((b[0].cut_offset_ms, b[1].cut_offset_ms), (5_000, 6_000));
        assert_eq!(
            (b[0].kind, b[1].kind),
            (BoundaryKind::DropStart, BoundaryKind::DropEnd)
        );
        assert_eq!(b[0].track_index, b[1].track_index);
        assert_eq!((b[2].cut_offset_ms, b[3].cut_offset_ms), (9_000, 10_000));
        assert_eq!(
            (b[2].kind, b[3].kind),
            (BoundaryKind::DropStart, BoundaryKind::DropEnd)
        );
    }

    #[test]
    fn policies_differ_per_run() {
        let runs = fx();
        let f = boundaries_from_silences(&runs, AbsorbPolicy::Forward);
        let b = boundaries_from_silences(&runs, AbsorbPolicy::Backward);
        let d = boundaries_from_silences(&runs, AbsorbPolicy::Drop);
        assert_ne!(f, b);
        assert_ne!(f, d);
        assert_ne!(b, d);
    }

    #[test]
    fn no_runs_emits_no_boundaries() {
        for policy in [
            AbsorbPolicy::Forward,
            AbsorbPolicy::Backward,
            AbsorbPolicy::Drop,
        ] {
            assert!(boundaries_from_silences(&[], policy).is_empty());
        }
    }

    #[test]
    fn single_run_spanning_whole_clip() {
        // Degenerate input: one silence covering the entire clip. Forward /
        // Backward still emit one cut, Drop emits a paired exclusion.
        let runs = [SilenceRun {
            start_ms: 0,
            end_ms: 60_000,
        }];
        let f = boundaries_from_silences(&runs, AbsorbPolicy::Forward);
        assert_eq!(
            f,
            vec![Boundary {
                track_index: 1,
                cut_offset_ms: 60_000,
                kind: BoundaryKind::Cut
            }]
        );
        let b = boundaries_from_silences(&runs, AbsorbPolicy::Backward);
        assert_eq!(
            b,
            vec![Boundary {
                track_index: 1,
                cut_offset_ms: 0,
                kind: BoundaryKind::Cut
            }]
        );
        let d = boundaries_from_silences(&runs, AbsorbPolicy::Drop);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].cut_offset_ms, 0);
        assert_eq!(d[1].cut_offset_ms, 60_000);
    }

    #[test]
    fn silence_at_t_zero() {
        let runs = [
            SilenceRun {
                start_ms: 0,
                end_ms: 1_500,
            },
            SilenceRun {
                start_ms: 30_000,
                end_ms: 31_000,
            },
        ];
        let f = boundaries_from_silences(&runs, AbsorbPolicy::Forward);
        assert_eq!(f[0].cut_offset_ms, 1_500);
        let b = boundaries_from_silences(&runs, AbsorbPolicy::Backward);
        assert_eq!(b[0].cut_offset_ms, 0);
    }

    #[test]
    fn silence_at_clip_end() {
        let runs = [
            SilenceRun {
                start_ms: 10_000,
                end_ms: 11_000,
            },
            SilenceRun {
                start_ms: 59_500,
                end_ms: 60_000,
            },
        ];
        let f = boundaries_from_silences(&runs, AbsorbPolicy::Forward);
        assert_eq!(f.last().unwrap().cut_offset_ms, 60_000);
        let b = boundaries_from_silences(&runs, AbsorbPolicy::Backward);
        assert_eq!(b.last().unwrap().cut_offset_ms, 59_500);
    }

    #[test]
    fn overlapping_runs_still_yield_per_run_boundaries() {
        // The pure function is total over its input — guard against malformed callers.
        let runs = [
            SilenceRun {
                start_ms: 5_000,
                end_ms: 7_000,
            },
            SilenceRun {
                start_ms: 6_000,
                end_ms: 8_000,
            },
        ];
        let f = boundaries_from_silences(&runs, AbsorbPolicy::Forward);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].cut_offset_ms, 7_000);
        assert_eq!(f[1].cut_offset_ms, 8_000);
        assert_ne!(f[0].track_index, f[1].track_index);
    }

    /// Verify detect_silences delegates correctly: [1s sine][1s silence][1s sine]
    /// at 16 kHz should yield exactly one SilenceRun.
    #[test]
    fn detect_silence_finds_one_run_in_synthetic_stream() {
        use crate::codecs::silence::detect_silence;
        use crate::codecs::{PcmFrame, StreamInfo};

        let sr = 16_000u32;
        let n = sr as usize;
        let sine: Vec<f32> = (0..n)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin())
            .collect();
        let frames = vec![
            PcmFrame {
                frames: n,
                samples: sine.clone(),
            },
            PcmFrame {
                frames: n,
                samples: vec![0.0; n],
            },
            PcmFrame {
                frames: n,
                samples: sine,
            },
        ];
        let info = StreamInfo {
            sample_rate: sr,
            channels: 1,
            duration_sec: 3.0,
            codec: "wav",
        };
        let runs = detect_silence(frames.into_iter(), &info, -45.0, 500);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].start_ms >= 950 && runs[0].start_ms <= 1050);
        assert!(runs[0].end_ms >= 1950 && runs[0].end_ms <= 2050);
    }
}
