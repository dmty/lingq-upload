use std::path::Path;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::{resolve_ffmpeg_bin, AudioError, STDERR_CAPTURE_BYTES};

/// How a silent chapter-divider is folded into its neighbour tracks.
///
/// `Forward` is the legacy behaviour and the default for newly created
/// projects: every cut lands at the END of the silent run, so the silence
/// glues onto the NEXT chapter. `Backward` cuts at the START of the silence
/// (silence stays with the PREVIOUS chapter). `Drop` emits a paired (start,
/// end) so the silent run is excised from both sides.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default,
)]
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
            silence_db: -30.0,
            min_silence_ms: 500,
            absorb: AbsorbPolicy::Forward,
        }
    }
}

/// A single cut point in the source audio. `track_index` is the 0-based index
/// of the chapter that BEGINS at this offset (so the first boundary's offset
/// is the start of track 1, not 0). `cut_offset_ms` is sample-accurate to
/// within ±50ms tolerance — see `golden_offsets.json` for the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Boundary {
    pub track_index: usize,
    pub cut_offset_ms: u32,
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
    let mut next_track: usize = 1;
    for run in runs {
        match policy {
            AbsorbPolicy::Forward => {
                out.push(Boundary {
                    track_index: next_track,
                    cut_offset_ms: run.end_ms,
                });
                next_track += 1;
            }
            AbsorbPolicy::Backward => {
                out.push(Boundary {
                    track_index: next_track,
                    cut_offset_ms: run.start_ms,
                });
                next_track += 1;
            }
            AbsorbPolicy::Drop => {
                // Two-cut form: an exclusion zone bracketed by start and end.
                // Track index is the same for both ends so callers can pair them.
                out.push(Boundary {
                    track_index: next_track,
                    cut_offset_ms: run.start_ms,
                });
                out.push(Boundary {
                    track_index: next_track,
                    cut_offset_ms: run.end_ms,
                });
                next_track += 1;
            }
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
        .args([
            "-hide_banner",
            "-nostats",
            "-v",
            "info",
            "-i",
        ])
        .arg(path)
        .args(["-af", &af, "-f", "null", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| CarveError::Audio(AudioError::Io(e.to_string())))?;

    let mut buf = Vec::new();
    if let Some(mut err) = child.stderr.take() {
        err.read_to_end(&mut buf)
            .await
            .map_err(|e| CarveError::Audio(AudioError::Io(e.to_string())))?;
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
    parse_silencedetect(&log)
}

fn parse_silencedetect(log: &str) -> Result<Vec<SilenceRun>, CarveError> {
    let mut runs: Vec<SilenceRun> = Vec::new();
    let mut pending_start: Option<u32> = None;
    for line in log.lines() {
        if let Some(rest) = line.split("silence_start:").nth(1) {
            let val = rest.split_whitespace().next().unwrap_or("");
            let secs: f64 = val
                .parse()
                .map_err(|e| CarveError::Parse(format!("silence_start {val:?}: {e}")))?;
            pending_start = Some(seconds_to_ms(secs));
        } else if let Some(rest) = line.split("silence_end:").nth(1) {
            let val = rest.split_whitespace().next().unwrap_or("");
            let secs: f64 = val
                .parse()
                .map_err(|e| CarveError::Parse(format!("silence_end {val:?}: {e}")))?;
            let end_ms = seconds_to_ms(secs);
            if let Some(start_ms) = pending_start.take() {
                if end_ms > start_ms {
                    runs.push(SilenceRun { start_ms, end_ms });
                }
            }
        }
    }
    Ok(runs)
}

fn seconds_to_ms(secs: f64) -> u32 {
    if secs.is_sign_negative() || !secs.is_finite() {
        return 0;
    }
    (secs * 1000.0).round().min(u32::MAX as f64) as u32
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
        assert_eq!(b[1].cut_offset_ms, 10_000);
    }

    #[test]
    fn backward_cuts_at_silence_start() {
        let b = boundaries_from_silences(&fx(), AbsorbPolicy::Backward);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].cut_offset_ms, 5_000);
        assert_eq!(b[1].cut_offset_ms, 9_000);
    }

    #[test]
    fn drop_emits_paired_cuts() {
        let b = boundaries_from_silences(&fx(), AbsorbPolicy::Drop);
        assert_eq!(b.len(), 4);
        assert_eq!((b[0].cut_offset_ms, b[1].cut_offset_ms), (5_000, 6_000));
        assert_eq!((b[2].cut_offset_ms, b[3].cut_offset_ms), (9_000, 10_000));
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
    fn parses_silencedetect_log() {
        let log = "\
[silencedetect @ 0xdead] silence_start: 5.001
[silencedetect @ 0xdead] silence_end: 6.000 | silence_duration: 0.999
[silencedetect @ 0xdead] silence_start: 9.0
[silencedetect @ 0xdead] silence_end: 10.0 | silence_duration: 1.0
";
        let runs = parse_silencedetect(log).expect("parse");
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].start_ms, 5_001);
        assert_eq!(runs[0].end_ms, 6_000);
    }

    #[test]
    fn unmatched_start_is_dropped() {
        let log = "\
[silencedetect] silence_start: 1.0
[silencedetect] silence_start: 5.0
[silencedetect] silence_end: 6.0
";
        let runs = parse_silencedetect(log).expect("parse");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].start_ms, 5_000);
    }
}
