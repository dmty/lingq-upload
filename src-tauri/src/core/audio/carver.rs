use std::path::Path;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::{probe_duration, resolve_ffmpeg_bin, AudioError};

const STDERR_CAPTURE_BYTES: usize = 4 * 1024;
const MAX_STDERR_PARSE_BYTES: usize = 4 * 1024 * 1024;

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
    #[error("audio probe: {0}")]
    Audio(#[from] AudioError),
    #[error("silencedetect parse: {0}")]
    Parse(String),
    #[error("silencedetect stderr exceeded {max} byte cap ({bytes} read)")]
    StderrTooLarge { bytes: usize, max: usize },
}

/// Map detected silence runs into per-policy chapter boundaries.
///
/// Pure function — no IO. The `carve` entrypoint runs `ffmpeg silencedetect`
/// and feeds the parsed runs in here.
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
    let bin = resolve_ffmpeg_bin().map_err(CarveError::Audio)?;
    let min_d = format!("{:.3}", (min_silence_ms as f64) / 1000.0);
    let af = format!("silencedetect=noise={silence_db}dB:d={min_d}");
    let mut child = Command::new(&bin)
        .args(["-hide_banner", "-nostats", "-v", "info", "-i"])
        .arg(path)
        .args(["-af", &af, "-f", "null", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| CarveError::Audio(AudioError::Io(e.to_string())))?;

    let mut buf = Vec::new();
    if let Some(err) = child.stderr.take() {
        // Read one byte past the cap so an over-cap buffer is detectable.
        let mut bounded = err.take(MAX_STDERR_PARSE_BYTES as u64 + 1);
        bounded
            .read_to_end(&mut buf)
            .await
            .map_err(|e| CarveError::Audio(AudioError::Io(e.to_string())))?;
    }
    if buf.len() > MAX_STDERR_PARSE_BYTES {
        // Reap the child so we don't leak a zombie on the early return.
        let _ = child.kill().await;
        return Err(CarveError::StderrTooLarge {
            bytes: buf.len(),
            max: MAX_STDERR_PARSE_BYTES,
        });
    }
    let status = child
        .wait()
        .await
        .map_err(|e| CarveError::Audio(AudioError::Io(e.to_string())))?;
    if !status.success() {
        let tail_start = buf.len().saturating_sub(STDERR_CAPTURE_BYTES);
        let stderr = String::from_utf8_lossy(&buf[tail_start..]).into_owned();
        return Err(CarveError::Audio(AudioError::FfmpegFailed {
            status: status.code().unwrap_or(-1),
            stderr,
        }));
    }
    let log = String::from_utf8_lossy(&buf);
    let (mut runs, pending_start) = parse_silencedetect(&log)?;
    // Older ffmpeg emits no silence_end when the stream ends mid-silence;
    // synthesize one at stream duration so the final boundary survives.
    if let Some(start_ms) = pending_start {
        let dur = probe_duration(path).await.map_err(CarveError::Audio)?;
        let end_ms = seconds_to_ms(dur)?;
        if end_ms > start_ms {
            runs.push(SilenceRun { start_ms, end_ms });
        }
    }
    Ok(runs)
}

/// Returns parsed runs plus any silence_start left unmatched at EOF.
fn parse_silencedetect(log: &str) -> Result<(Vec<SilenceRun>, Option<u32>), CarveError> {
    let mut runs: Vec<SilenceRun> = Vec::new();
    let mut pending_start: Option<u32> = None;
    for line in log.lines() {
        if let Some(rest) = line.split("silence_start:").nth(1) {
            let val = rest.split_whitespace().next().unwrap_or("");
            let secs: f64 = val
                .parse()
                .map_err(|e| CarveError::Parse(format!("silence_start {val:?}: {e}")))?;
            pending_start = Some(seconds_to_ms(secs)?);
        } else if let Some(rest) = line.split("silence_end:").nth(1) {
            let val = rest.split_whitespace().next().unwrap_or("");
            let secs: f64 = val
                .parse()
                .map_err(|e| CarveError::Parse(format!("silence_end {val:?}: {e}")))?;
            let end_ms = seconds_to_ms(secs)?;
            if let Some(start_ms) = pending_start.take() {
                if end_ms > start_ms {
                    runs.push(SilenceRun { start_ms, end_ms });
                }
            }
        }
    }
    Ok((runs, pending_start))
}

fn seconds_to_ms(secs: f64) -> Result<u32, CarveError> {
    if !secs.is_finite() {
        return Err(CarveError::Parse(format!("non-finite seconds: {secs}")));
    }
    if secs.is_sign_negative() {
        return Ok(0);
    }
    let ms = (secs * 1000.0).round();
    if ms > u32::MAX as f64 {
        return Err(CarveError::Parse(format!(
            "seconds {secs} overflows u32 ms"
        )));
    }
    Ok(ms as u32)
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
        // ffmpeg silencedetect won't emit this, but the pure function is
        // total over its input — guard against malformed callers.
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

    #[test]
    fn parses_silencedetect_log() {
        let log = "\
[silencedetect @ 0xdead] silence_start: 5.001
[silencedetect @ 0xdead] silence_end: 6.000 | silence_duration: 0.999
[silencedetect @ 0xdead] silence_start: 9.0
[silencedetect @ 0xdead] silence_end: 10.0 | silence_duration: 1.0
";
        let (runs, pending) = parse_silencedetect(log).expect("parse");
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].start_ms, 5_001);
        assert_eq!(runs[0].end_ms, 6_000);
        assert_eq!(pending, None);
    }

    #[test]
    fn superseded_start_is_dropped() {
        let log = "\
[silencedetect] silence_start: 1.0
[silencedetect] silence_start: 5.0
[silencedetect] silence_end: 6.0
";
        let (runs, pending) = parse_silencedetect(log).expect("parse");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].start_ms, 5_000);
        assert_eq!(pending, None);
    }

    #[test]
    fn trailing_start_surfaces_as_pending() {
        let log = "\
[silencedetect] silence_start: 1.0
[silencedetect] silence_end: 2.0
[silencedetect] silence_start: 7.04
";
        let (runs, pending) = parse_silencedetect(log).expect("parse");
        assert_eq!(runs.len(), 1);
        assert_eq!(pending, Some(7_040));
    }

    #[test]
    fn seconds_to_ms_negative_clamps_to_zero() {
        assert_eq!(seconds_to_ms(-1.0).unwrap(), 0);
    }

    #[test]
    fn seconds_to_ms_overflow_errors() {
        assert!(matches!(seconds_to_ms(1.0e12), Err(CarveError::Parse(_))));
    }

    #[test]
    fn seconds_to_ms_nan_errors() {
        assert!(matches!(seconds_to_ms(f64::NAN), Err(CarveError::Parse(_))));
    }

    #[test]
    fn stderr_parse_cap_is_four_mib() {
        assert_eq!(MAX_STDERR_PARSE_BYTES, 4 * 1024 * 1024);
    }

    #[tokio::test]
    async fn read_bounded_by_max_stderr_parse_bytes() {
        // Stand-in for ffmpeg stderr: a stream larger than the cap. We reuse
        // the same `AsyncRead::take(N+1).read_to_end` pattern as `carve` so
        // the test exercises the actual bound, not a copy of the constant.
        use tokio::io::AsyncReadExt;
        let oversized = vec![b'x'; MAX_STDERR_PARSE_BYTES + 4096];
        let mut bounded = (&oversized[..]).take(MAX_STDERR_PARSE_BYTES as u64 + 1);
        let mut buf = Vec::new();
        bounded.read_to_end(&mut buf).await.unwrap();
        assert!(buf.len() > MAX_STDERR_PARSE_BYTES);
        assert_eq!(buf.len(), MAX_STDERR_PARSE_BYTES + 1);

        let err = CarveError::StderrTooLarge {
            bytes: buf.len(),
            max: MAX_STDERR_PARSE_BYTES,
        };
        assert!(matches!(err, CarveError::StderrTooLarge { .. }));
    }
}
